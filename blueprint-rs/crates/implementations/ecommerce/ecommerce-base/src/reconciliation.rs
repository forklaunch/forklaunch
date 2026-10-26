//! Explicit repair from authenticated provider evidence, separate from TS transitions.
use crate::{
    BaseEcommerceService,
    store::{EcommerceStore, Record, RecordKind, not_found, storage_error},
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::{PaymentDto, PaymentStatus};
use serde::{Deserialize, Serialize};
use std::future::Future;

/// Trusted server-side evidence. Never accept this structure directly from a caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedCapture {
    pub provider: String,
    pub provider_ref: String,
    pub capture_id: String,
    pub amount_cents: i64,
    pub currency: String,
}
#[derive(Debug, Clone, PartialEq)]
pub struct PaymentReconciliationAudit {
    pub payment_id: String,
    pub previous_status: PaymentStatus,
    pub evidence: VerifiedCapture,
    pub actor: AuditContext,
}
/// Implementations must compare the inspected snapshot and atomically audit repair.
pub trait PaymentReconciliationStore: EcommerceStore {
    /// # Errors
    /// Rejects stale/missing records, mismatched evidence or missing actor context.
    fn record_verified_capture(
        &self,
        expected: PaymentDto,
        evidence: VerifiedCapture,
        actor: AuditContext,
    ) -> impl Future<Output = BlueprintResult<PaymentDto>> + Send;
}
impl<S: PaymentReconciliationStore> BaseEcommerceService<S> {
    /// Records already-verified capture evidence without making a provider request.
    /// This trusted server API must not be exposed as an unverified HTTP payload.
    /// # Errors
    /// Rejects stale records, invalid evidence and audit/storage failures.
    pub async fn record_verified_capture(
        &self,
        expected: PaymentDto,
        evidence: VerifiedCapture,
        actor: AuditContext,
    ) -> BlueprintResult<PaymentDto> {
        self.store
            .record_verified_capture(expected, evidence, actor)
            .await
    }
}
pub(crate) fn validate(
    expected: &PaymentDto,
    evidence: &VerifiedCapture,
    actor: &AuditContext,
) -> BlueprintResult<()> {
    if !valid_id(&evidence.capture_id) {
        return Err(invalid_evidence());
    }
    validate_payment_evidence(
        expected,
        &evidence.provider,
        &evidence.provider_ref,
        evidence.amount_cents,
        &evidence.currency,
        actor,
    )
}
pub(crate) fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
fn invalid_evidence() -> BlueprintError {
    BlueprintError::new(
        "ecommerce_invalid_reconciliation",
        "Provider evidence or reconciliation actor is invalid",
    )
}
pub(crate) fn validate_payment_evidence(
    expected: &PaymentDto,
    provider: &str,
    provider_ref: &str,
    amount_cents: i64,
    currency: &str,
    actor: &AuditContext,
) -> BlueprintResult<()> {
    if actor
        .actor_id
        .as_deref()
        .is_none_or(|value| !valid_id(value))
        || !valid_id(&actor.correlation_id)
        || !valid_id(provider)
        || !valid_id(provider_ref)
        || amount_cents <= 0
        || expected.provider_ref.as_deref() != Some(provider_ref)
        || expected.amount_cents != amount_cents
        || !expected.currency.eq_ignore_ascii_case(currency)
    {
        return Err(invalid_evidence());
    }
    Ok(())
}
pub(crate) fn repair(current: &PaymentDto, expected: &PaymentDto) -> BlueprintResult<PaymentDto> {
    if current != expected {
        return Err(BlueprintError::new(
            "ecommerce_reconciliation_conflict",
            "Payment changed while provider evidence was being inspected",
        ));
    }
    let mut payment = current.clone();
    if payment.status != PaymentStatus::Succeeded {
        payment.status = PaymentStatus::Succeeded;
        payment.updated_at = Some(crate::timestamp());
    }
    Ok(payment)
}
impl PaymentReconciliationStore for crate::store::InMemoryEcommerceStore {
    async fn record_verified_capture(
        &self,
        expected: PaymentDto,
        evidence: VerifiedCapture,
        actor: AuditContext,
    ) -> BlueprintResult<PaymentDto> {
        validate(&expected, &evidence, &actor)?;
        let mut records = self.records.lock().map_err(storage_error)?;
        let key = (RecordKind::Payment, expected.id.clone());
        let Some(Record::Payment(current)) = records.get(&key) else {
            return Err(not_found(RecordKind::Payment));
        };
        if self
            .payment_cancellation_audit
            .lock()
            .map_err(storage_error)?
            .iter()
            .any(|audit| {
                audit.payment_id == expected.id
                    && Some(&audit.evidence.provider_ref) == expected.provider_ref.as_ref()
            })
        {
            return Err(crate::cancellation::conflict());
        }
        let payment = repair(current, &expected)?;
        let mut audit = self
            .payment_reconciliation_audit
            .lock()
            .map_err(storage_error)?;
        audit.push(PaymentReconciliationAudit {
            payment_id: payment.id.clone(),
            previous_status: current.status,
            evidence,
            actor,
        });
        records.insert(key, Record::Payment(payment.clone()));
        Ok(payment)
    }
}
impl crate::store::InMemoryEcommerceStore {
    /// Development-only inspection of recorded repair decisions.
    /// # Errors
    /// Returns a storage error if the in-memory audit lock was poisoned.
    pub fn payment_reconciliation_audits(
        &self,
    ) -> BlueprintResult<Vec<PaymentReconciliationAudit>> {
        Ok(self
            .payment_reconciliation_audit
            .lock()
            .map_err(storage_error)?
            .clone())
    }
}
