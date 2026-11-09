//! Operator adoption of an existing full capture; never a provider financial action.
use super::super::storage_error;
use super::scoped_cancellation::locked_binding_matches;
use super::{PaymentProviderBinding, PostgresEcommerceStore};
use crate::{
    cancellation::conflict,
    reconciliation::{VerifiedCapture, repair, validate},
    store::Record,
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::{PaymentDto, PaymentStatus};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction, types::Json};
use uuid::Uuid;

/// Opaque exact payment, noncancelled order and administrative binding snapshot.
pub struct CapturedPaymentSnapshot {
    binding: PaymentProviderBinding,
    payment: PaymentDto,
    order_version: String,
    order_status: String,
}
impl CapturedPaymentSnapshot {
    #[must_use]
    pub const fn payment(&self) -> &PaymentDto {
        &self.payment
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
/// Durable operator receipt; no provider retry/settlement authority.
pub struct ScopedCaptureReceipt {
    pub id: String,
    pub payment_id: String,
    pub order_id: String,
    pub binding_generation: String,
    pub provider_ref: String,
    pub capture_id: String,
    pub balance_transaction_id: String,
}
/// A successful COMMIT is never reported as rolled back when evidence expires.
pub enum ScopedCaptureOutcome {
    Recorded {
        receipt: ScopedCaptureReceipt,
        payment: PaymentDto,
    },
    RecordedEvidenceNoLongerCurrent {
        receipt: ScopedCaptureReceipt,
    },
}
fn ambiguous_commit() -> BlueprintError {
    BlueprintError::new(
        "ecommerce_capture_commit_unknown",
        "Capture commit outcome is unknown; inspect the supplied receipt ID",
    )
}
fn matches_payment(binding: &PaymentProviderBinding, payment: &PaymentDto) -> bool {
    binding.payment_id == payment.id
        && binding.order_id == payment.order_id
        && payment.provider_ref.as_deref() == Some(binding.provider_ref.as_str())
        && binding.amount_cents == payment.amount_cents
        && binding.currency == payment.currency.to_ascii_uppercase()
        && payment.status != PaymentStatus::Succeeded
}
impl PostgresEcommerceStore {
    /// Snapshots a pending/failed payment and a known noncancelled order without
    /// selecting or decrypting order addresses. No provider proof is inferred.
    /// # Errors
    /// Denies foreign, substituted, succeeded or missing local relationships.
    pub async fn captured_payment_snapshot(
        &self,
        binding: &PaymentProviderBinding,
    ) -> BlueprintResult<CapturedPaymentSnapshot> {
        if binding.tenant_id != self.tenant.id() || binding.scope.provider != "stripe" {
            return Err(conflict());
        }
        let mut tx = self.transaction().await?;
        let row:Option<(Json<PaymentDto>,String,String,String)>=sqlx::query_as("SELECT p.data,o.xmin::text,b.generation::text,o.data->>'status' FROM fl_ecommerce.records p JOIN fl_ecommerce.records o ON o.tenant_id=p.tenant_id AND o.kind='order' AND o.id=p.data->>'orderId' JOIN fl_ecommerce.payment_provider_bindings b ON b.tenant_id=p.tenant_id AND b.payment_id=p.id WHERE p.tenant_id=$1 AND p.kind='payment' AND p.id=$2 AND p.data->>'id'=p.id AND o.data->>'id'=o.id AND o.data->>'status' IN ('pending','paid','fulfilled','shipped','delivered')")
            .bind(self.tenant.id()).bind(&binding.payment_id).fetch_optional(&mut *tx).await.map_err(storage_error)?;
        let (Json(payment), order_version, generation, order_status) = row.ok_or_else(conflict)?;
        if generation != binding.generation || !matches_payment(binding, &payment) {
            return Err(conflict());
        }
        // A generation can only be minted by provisioning, but compare its entire
        // current row as well so raw administrative edits cannot substitute scope.
        if !locked_binding_matches(&mut tx, binding, false).await? {
            return Err(conflict());
        }
        tx.commit().await.map_err(storage_error)?;
        Ok(CapturedPaymentSnapshot {
            binding: binding.clone(),
            payment,
            order_version,
            order_status,
        })
    }
    /// Trusted operator API for already authenticated full-capture evidence.
    /// Locks payment, order, then binding; atomically records payment/audit/receipt.
    /// Does not transition an order, clear references or change provider money.
    /// Retain receipt UUID before calling; uncertain COMMIT requires inspection.
    /// # Errors
    /// Denies stale snapshots, prior capture/cancellation history, invalid evidence,
    /// expired context or audit failure. COMMIT errors explicitly remain ambiguous.
    #[allow(clippy::too_many_arguments)] // Keep verified capture and balance linkage explicit.
    pub async fn record_scoped_capture<F>(
        &self,
        snapshot: CapturedPaymentSnapshot,
        evidence: VerifiedCapture,
        balance_transaction_id: &str,
        actor: AuditContext,
        receipt_id: Uuid,
        fresh: F,
    ) -> BlueprintResult<ScopedCaptureOutcome>
    where
        F: Fn() -> BlueprintResult<()> + Send + Sync,
    {
        validate(&snapshot.payment, &evidence, &actor)?;
        if receipt_id.is_nil()
            || snapshot.binding.tenant_id != self.tenant.id()
            || evidence.provider != "stripe"
            || !crate::reconciliation::valid_id(balance_transaction_id)
            || actor
                .actor_id
                .as_deref()
                .is_none_or(|s| s.trim().is_empty())
            || actor.correlation_id.trim().is_empty()
        {
            return Err(conflict());
        }
        fresh()?;
        let mut tx = self.transaction().await?;
        let operation = self
            .scoped_capture_write(
                &mut tx,
                &snapshot,
                &evidence,
                balance_transaction_id,
                &actor,
                receipt_id,
                &fresh,
            )
            .await;
        let (receipt, payment) = match operation {
            Ok(value) => value,
            Err(error) => {
                tx.rollback().await.map_err(storage_error)?;
                return Err(error);
            }
        };
        // No awaited operation separates this final freshness check and COMMIT.
        if let Err(error) = fresh() {
            tx.rollback().await.map_err(storage_error)?;
            return Err(error);
        }
        tx.commit().await.map_err(|_| ambiguous_commit())?;
        if fresh().is_err() {
            return Ok(ScopedCaptureOutcome::RecordedEvidenceNoLongerCurrent { receipt });
        }
        Ok(ScopedCaptureOutcome::Recorded { receipt, payment })
    }
    #[allow(clippy::too_many_arguments)] // Single transaction carries the full immutable proof.
    async fn scoped_capture_write<F>(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        snapshot: &CapturedPaymentSnapshot,
        evidence: &VerifiedCapture,
        balance_transaction_id: &str,
        actor: &AuditContext,
        receipt_id: Uuid,
        fresh: &F,
    ) -> BlueprintResult<(ScopedCaptureReceipt, PaymentDto)>
    where
        F: Fn() -> BlueprintResult<()> + Send + Sync,
    {
        let binding = &snapshot.binding;
        let current:Option<Json<PaymentDto>>=sqlx::query_scalar("SELECT data FROM fl_ecommerce.records WHERE tenant_id=$1 AND kind='payment' AND id=$2 FOR UPDATE")
            .bind(self.tenant.id()).bind(&binding.payment_id).fetch_optional(&mut **tx).await.map_err(storage_error)?;
        let Json(current) = current.ok_or_else(conflict)?;
        if !matches_payment(binding, &current) {
            return Err(conflict());
        }
        let order:Option<(String,String,String)>=sqlx::query_as("SELECT data->>'id',data->>'status',xmin::text FROM fl_ecommerce.records WHERE tenant_id=$1 AND kind='order' AND id=$2 FOR UPDATE")
            .bind(self.tenant.id()).bind(&binding.order_id).fetch_optional(&mut **tx).await.map_err(storage_error)?;
        if order
            != Some((
                binding.order_id.clone(),
                snapshot.order_status.clone(),
                snapshot.order_version.clone(),
            ))
            || !locked_binding_matches(tx, binding, true).await?
        {
            return Err(conflict());
        }
        let captured:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fl_ecommerce.payment_reconciliation_audit WHERE tenant_id=$1 AND payment_id=$2 AND provider_ref=$3)")
            .bind(self.tenant.id()).bind(&binding.payment_id).bind(&binding.provider_ref).fetch_one(&mut **tx).await.map_err(storage_error)?;
        if captured
            || super::super::cancellation::is_canceled(tx, self.tenant.id(), &current).await?
        {
            return Err(conflict());
        }
        fresh()?;
        let payment = repair(&current, &snapshot.payment)?;
        let audit_id = Uuid::new_v4();
        sqlx::query("INSERT INTO fl_ecommerce.payment_reconciliation_audit(id,tenant_id,payment_id,previous_status,provider,provider_ref,capture_id,amount_cents,currency,actor_id,correlation_id) VALUES($1::uuid,$2,$3,$4,'stripe',$5,$6,$7,$8,$9,$10)")
            .bind(audit_id.to_string()).bind(self.tenant.id()).bind(&payment.id)
            .bind(serde_json::to_value(current.status).map_err(storage_error)?.as_str().ok_or_else(conflict)?)
            .bind(&evidence.provider_ref).bind(&evidence.capture_id).bind(evidence.amount_cents).bind(&evidence.currency).bind(&actor.actor_id).bind(&actor.correlation_id)
            .execute(&mut **tx).await.map_err(storage_error)?;
        sqlx::query("INSERT INTO fl_ecommerce.scoped_payment_capture_receipts(id,tenant_id,reconciliation_audit_id,payment_id,order_id,order_version,binding_generation,provider,deployment_label,merchant_id,livemode,provider_ref,amount_cents,currency,capture_id,actor_id,correlation_id,balance_transaction_id) VALUES($1::uuid,$2,$3::uuid,$4,$5,$6,$7::uuid,'stripe',$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)")
            .bind(receipt_id.to_string()).bind(self.tenant.id()).bind(audit_id.to_string()).bind(&payment.id).bind(&binding.order_id).bind(&snapshot.order_version).bind(&binding.generation)
            .bind(&binding.scope.deployment_label).bind(&binding.scope.merchant_id).bind(binding.scope.livemode).bind(&binding.provider_ref).bind(binding.amount_cents).bind(&binding.currency).bind(&evidence.capture_id).bind(&actor.actor_id).bind(&actor.correlation_id).bind(balance_transaction_id)
            .execute(&mut **tx).await.map_err(storage_error)?;
        if payment != current {
            let data = self.encode(&Record::Payment(payment.clone()))?;
            sqlx::query("UPDATE fl_ecommerce.records SET data=$3 WHERE tenant_id=$1 AND kind='payment' AND id=$2")
                .bind(self.tenant.id()).bind(&payment.id).bind(Json(data)).execute(&mut **tx).await.map_err(storage_error)?;
        }
        Ok((
            ScopedCaptureReceipt {
                id: receipt_id.to_string(),
                payment_id: payment.id.clone(),
                order_id: binding.order_id.clone(),
                binding_generation: binding.generation.clone(),
                provider_ref: binding.provider_ref.clone(),
                capture_id: evidence.capture_id.clone(),
                balance_transaction_id: balance_transaction_id.into(),
            },
            payment,
        ))
    }
    /// Historical tenant-scoped receipt lookup after successful or uncertain COMMIT.
    /// Absence is not permission to retry a provider action.
    /// # Errors
    /// Returns sanitized storage failures.
    pub async fn scoped_capture_receipt(
        &self,
        id: Uuid,
    ) -> BlueprintResult<Option<ScopedCaptureReceipt>> {
        let mut tx = self.transaction().await?;
        let row:Option<(String,String,String,String,String,String)>=sqlx::query_as("SELECT payment_id,order_id,binding_generation::text,provider_ref,capture_id,balance_transaction_id FROM fl_ecommerce.scoped_payment_capture_receipts WHERE tenant_id=$1 AND id=$2::uuid")
            .bind(self.tenant.id()).bind(id.to_string()).fetch_optional(&mut *tx).await.map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(row.map(
            |(
                payment_id,
                order_id,
                binding_generation,
                provider_ref,
                capture_id,
                balance_transaction_id,
            )| {
                ScopedCaptureReceipt {
                    id: id.to_string(),
                    payment_id,
                    order_id,
                    binding_generation,
                    provider_ref,
                    capture_id,
                    balance_transaction_id,
                }
            },
        ))
    }
}
