//! Explicit operator repair of a cancelled order's terminal no-charge payment.
use super::super::storage_error;
use super::{PaymentProviderBinding, PostgresEcommerceStore};
use crate::{
    cancellation::{VerifiedCancellation, conflict, repair, validate},
    store::Record,
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::{PaymentDto, PaymentStatus};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction, types::Json};
use uuid::Uuid;

/// Exact tenant/binding/payment/cancelled-order snapshot. Never caller-deserialized.
pub struct CancelledOrderPaymentSnapshot {
    binding: PaymentProviderBinding,
    payment: PaymentDto,
    order_version: String,
}
impl CancelledOrderPaymentSnapshot {
    #[must_use]
    pub const fn payment(&self) -> &PaymentDto {
        &self.payment
    }
}
/// Stable lookup key supplied before attempting COMMIT. The receipt records an
/// operator decision, not authority to retry provider operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScopedCancellationReceipt {
    pub id: String,
    pub payment_id: String,
    pub order_id: String,
    pub binding_generation: String,
    pub provider_ref: String,
}
/// Successful COMMIT is never represented as a rollback when the deadline elapses.
pub enum ScopedCancellationOutcome {
    Recorded {
        receipt: ScopedCancellationReceipt,
        payment: PaymentDto,
    },
    RecordedEvidenceNoLongerCurrent {
        receipt: ScopedCancellationReceipt,
    },
}
fn ambiguous_commit() -> BlueprintError {
    BlueprintError::new(
        "ecommerce_cancellation_commit_unknown",
        "Cancellation commit outcome is unknown; inspect the supplied receipt ID",
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
    /// Checks current cancelled-order identity without selecting/decrypting addresses.
    /// # Errors
    /// Rejects foreign/stale bindings, changed payment relations and succeeded payments.
    pub async fn cancelled_order_payment_snapshot(
        &self,
        binding: &PaymentProviderBinding,
    ) -> BlueprintResult<CancelledOrderPaymentSnapshot> {
        if binding.tenant_id != self.tenant.id() || binding.scope.provider != "stripe" {
            return Err(conflict());
        }
        let mut tx = self.transaction().await?;
        let row:Option<(Json<PaymentDto>,String,String)>=sqlx::query_as("SELECT p.data,o.xmin::text,b.generation::text FROM fl_ecommerce.records p JOIN fl_ecommerce.records o ON o.tenant_id=p.tenant_id AND o.kind='order' AND o.id=p.data->>'orderId' JOIN fl_ecommerce.payment_provider_bindings b ON b.tenant_id=p.tenant_id AND b.payment_id=p.id WHERE p.tenant_id=$1 AND p.kind='payment' AND p.id=$2 AND p.data->>'id'=p.id AND o.data->>'id'=o.id AND o.data->>'status'='cancelled'")
            .bind(self.tenant.id()).bind(&binding.payment_id).fetch_optional(&mut *tx).await.map_err(storage_error)?;
        let (Json(payment), order_version, generation) = row.ok_or_else(conflict)?;
        if generation != binding.generation || !matches_payment(binding, &payment) {
            return Err(conflict());
        }
        // A generation can only be minted by provisioning, but compare its entire
        // current row as well so raw administrative edits cannot substitute scope.
        if !locked_binding_matches(&mut tx, binding, false).await? {
            return Err(conflict());
        }
        tx.commit().await.map_err(storage_error)?;
        Ok(CancelledOrderPaymentSnapshot {
            binding: binding.clone(),
            payment,
            order_version,
        })
    }
    /// Trusted operator API. `evidence` must come from authenticated terminal
    /// provider reads; `fresh` must validate that same sealed context. Locks payment,
    /// order, then binding (matching provision); never holds locks over provider I/O.
    /// Receipt ID must be retained before invoking this method. COMMIT cancellation
    /// or transport errors can be ambiguous: inspect the receipt, never assume rollback.
    /// # Errors
    /// Rejects stale identities, noncancelled/succeeded/captured state, invalid evidence,
    /// expired context or failed audits. A COMMIT error has a distinct ambiguity code.
    pub async fn record_scoped_cancelled_order_cancellation<F>(
        &self,
        snapshot: CancelledOrderPaymentSnapshot,
        evidence: VerifiedCancellation,
        actor: AuditContext,
        receipt_id: Uuid,
        fresh: F,
    ) -> BlueprintResult<ScopedCancellationOutcome>
    where
        F: Fn() -> BlueprintResult<()> + Send + Sync,
    {
        validate(&snapshot.payment, &evidence, &actor)?;
        if receipt_id.is_nil()
            || snapshot.binding.tenant_id != self.tenant.id()
            || evidence.provider != "stripe"
            || evidence.canceled_at.is_none()
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
            .scoped_cancellation_write(&mut tx, &snapshot, &evidence, &actor, receipt_id, &fresh)
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
            return Ok(ScopedCancellationOutcome::RecordedEvidenceNoLongerCurrent { receipt });
        }
        Ok(ScopedCancellationOutcome::Recorded { receipt, payment })
    }
    async fn scoped_cancellation_write<F>(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        snapshot: &CancelledOrderPaymentSnapshot,
        evidence: &VerifiedCancellation,
        actor: &AuditContext,
        receipt_id: Uuid,
        fresh: &F,
    ) -> BlueprintResult<(ScopedCancellationReceipt, PaymentDto)>
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
                "cancelled".into(),
                snapshot.order_version.clone(),
            ))
            || !locked_binding_matches(tx, binding, true).await?
        {
            return Err(conflict());
        }
        let captured:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fl_ecommerce.payment_reconciliation_audit WHERE tenant_id=$1 AND payment_id=$2 AND provider_ref=$3)")
            .bind(self.tenant.id()).bind(&binding.payment_id).bind(&binding.provider_ref).fetch_one(&mut **tx).await.map_err(storage_error)?;
        if captured {
            return Err(conflict());
        }
        fresh()?;
        let payment = repair(&current, &snapshot.payment)?;
        let audit_id = Uuid::new_v4();
        sqlx::query("INSERT INTO fl_ecommerce.payment_cancellation_audit(id,tenant_id,payment_id,previous_status,provider,provider_ref,canceled_at,amount_cents,currency,actor_id,correlation_id) VALUES($1::uuid,$2,$3,$4,'stripe',$5,$6,$7,$8,$9,$10)")
            .bind(audit_id.to_string()).bind(self.tenant.id()).bind(&payment.id)
            .bind(serde_json::to_value(current.status).map_err(storage_error)?.as_str().ok_or_else(conflict)?)
            .bind(&evidence.provider_ref).bind(evidence.canceled_at).bind(evidence.amount_cents).bind(&evidence.currency).bind(&actor.actor_id).bind(&actor.correlation_id)
            .execute(&mut **tx).await.map_err(storage_error)?;
        sqlx::query("INSERT INTO fl_ecommerce.scoped_payment_cancellation_receipts(id,tenant_id,cancellation_audit_id,payment_id,order_id,order_version,binding_generation,provider,deployment_label,merchant_id,livemode,provider_ref,amount_cents,currency,canceled_at,actor_id,correlation_id) VALUES($1::uuid,$2,$3::uuid,$4,$5,$6,$7::uuid,'stripe',$8,$9,$10,$11,$12,$13,$14,$15,$16)")
            .bind(receipt_id.to_string()).bind(self.tenant.id()).bind(audit_id.to_string()).bind(&payment.id).bind(&binding.order_id).bind(&snapshot.order_version).bind(&binding.generation)
            .bind(&binding.scope.deployment_label).bind(&binding.scope.merchant_id).bind(binding.scope.livemode).bind(&binding.provider_ref).bind(binding.amount_cents).bind(&binding.currency).bind(evidence.canceled_at).bind(&actor.actor_id).bind(&actor.correlation_id)
            .execute(&mut **tx).await.map_err(storage_error)?;
        if payment != current {
            let data = self.encode(&Record::Payment(payment.clone()))?;
            sqlx::query("UPDATE fl_ecommerce.records SET data=$3 WHERE tenant_id=$1 AND kind='payment' AND id=$2")
                .bind(self.tenant.id()).bind(&payment.id).bind(Json(data)).execute(&mut **tx).await.map_err(storage_error)?;
        }
        Ok((
            ScopedCancellationReceipt {
                id: receipt_id.to_string(),
                payment_id: payment.id.clone(),
                order_id: binding.order_id.clone(),
                binding_generation: binding.generation.clone(),
                provider_ref: binding.provider_ref.clone(),
            },
            payment,
        ))
    }
    /// Tenant-scoped lookup after successful or ambiguous COMMIT. Absence is an
    /// observation, not permission to repeat a financial operation.
    /// # Errors
    /// Returns sanitized database errors.
    pub async fn scoped_cancellation_receipt(
        &self,
        id: Uuid,
    ) -> BlueprintResult<Option<ScopedCancellationReceipt>> {
        let mut tx = self.transaction().await?;
        let row:Option<(String,String,String,String)>=sqlx::query_as("SELECT payment_id,order_id,binding_generation::text,provider_ref FROM fl_ecommerce.scoped_payment_cancellation_receipts WHERE tenant_id=$1 AND id=$2::uuid")
            .bind(self.tenant.id()).bind(id.to_string()).fetch_optional(&mut *tx).await.map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(
            row.map(|(payment_id, order_id, binding_generation, provider_ref)| {
                ScopedCancellationReceipt {
                    id: id.to_string(),
                    payment_id,
                    order_id,
                    binding_generation,
                    provider_ref,
                }
            }),
        )
    }
}
type BindingRow = (
    String,
    String,
    String,
    String,
    bool,
    String,
    String,
    i64,
    String,
);

pub(super) async fn locked_binding_matches(
    tx: &mut Transaction<'_, Postgres>,
    binding: &PaymentProviderBinding,
    lock: bool,
) -> BlueprintResult<bool> {
    let query = format!(
        "SELECT generation::text,provider,deployment_label,merchant_id,livemode,provider_ref,order_id,amount_cents,currency FROM fl_ecommerce.payment_provider_bindings WHERE tenant_id=$1 AND payment_id=$2{}",
        if lock { " FOR UPDATE" } else { "" }
    );
    let row: Option<BindingRow> = sqlx::query_as(sqlx::AssertSqlSafe(query))
        .bind(&binding.tenant_id)
        .bind(&binding.payment_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage_error)?;
    Ok(row
        == Some((
            binding.generation.clone(),
            binding.scope.provider.clone(),
            binding.scope.deployment_label.clone(),
            binding.scope.merchant_id.clone(),
            binding.scope.livemode,
            binding.provider_ref.clone(),
            binding.order_id.clone(),
            binding.amount_cents,
            binding.currency.clone(),
        )))
}
