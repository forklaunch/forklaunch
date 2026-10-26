//! Audited terminal provider cancellation; never reopen a capture attempt.
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

/// Trusted evidence of terminal cancellation without a charge. Never deserialize
/// this directly from an untrusted request; obtain it through an authenticated
/// provider reader bound to the merchant account and local payment snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedCancellation {
    pub provider: String,
    pub provider_ref: String,
    /// Provider cancellation time, when explicitly supplied. Never substitute an
    /// order update time or the local observation time for a missing provider time.
    pub canceled_at: Option<i64>,
    pub amount_cents: i64,
    pub currency: String,
}
#[derive(Debug, Clone, PartialEq)]
pub struct PaymentCancellationAudit {
    pub payment_id: String,
    pub previous_status: PaymentStatus,
    pub evidence: VerifiedCancellation,
    pub actor: AuditContext,
}
/// Stores must also reject later confirmation/claim mutations and contradictory
/// capture repairs for the same payment/provider reference after cancellation.
pub trait PaymentCancellationStore: EcommerceStore {
    /// Compare the inspected snapshot and atomically record cancellation and audit.
    /// # Errors
    /// Rejects stale/contradictory records, invalid evidence and audit failures.
    fn record_verified_cancellation(
        &self,
        expected: PaymentDto,
        evidence: VerifiedCancellation,
        actor: AuditContext,
    ) -> impl Future<Output = BlueprintResult<PaymentDto>> + Send;
}
impl<S: PaymentCancellationStore> BaseEcommerceService<S> {
    /// Trusted server API; evidence must already be authenticated and terminal.
    /// # Errors
    /// Rejects stale/contradictory records, invalid evidence and audit failures.
    pub async fn record_verified_cancellation(
        &self,
        expected: PaymentDto,
        evidence: VerifiedCancellation,
        actor: AuditContext,
    ) -> BlueprintResult<PaymentDto> {
        self.store
            .record_verified_cancellation(expected, evidence, actor)
            .await
    }
}
pub(crate) fn conflict() -> BlueprintError {
    BlueprintError::new(
        "ecommerce_terminal_payment_conflict",
        "Payment has contradictory or terminal provider evidence",
    )
}
pub(crate) fn validate(
    expected: &PaymentDto,
    evidence: &VerifiedCancellation,
    actor: &AuditContext,
) -> BlueprintResult<()> {
    crate::reconciliation::validate_payment_evidence(
        expected,
        &evidence.provider,
        &evidence.provider_ref,
        evidence.amount_cents,
        &evidence.currency,
        actor,
    )?;
    if evidence.canceled_at.is_some_and(|timestamp| timestamp <= 0) {
        return Err(conflict());
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
    if payment.status != PaymentStatus::Failed {
        payment.status = PaymentStatus::Failed;
        payment.updated_at = Some(crate::timestamp());
    }
    Ok(payment)
}
impl PaymentCancellationStore for crate::store::InMemoryEcommerceStore {
    async fn record_verified_cancellation(
        &self,
        expected: PaymentDto,
        evidence: VerifiedCancellation,
        actor: AuditContext,
    ) -> BlueprintResult<PaymentDto> {
        validate(&expected, &evidence, &actor)?;
        let mut records = self.records.lock().map_err(storage_error)?;
        let key = (RecordKind::Payment, expected.id.clone());
        let Some(Record::Payment(current)) = records.get(&key) else {
            return Err(not_found(RecordKind::Payment));
        };
        if self
            .payment_reconciliation_audit
            .lock()
            .map_err(storage_error)?
            .iter()
            .any(|audit| {
                audit.payment_id == expected.id
                    && Some(&audit.evidence.provider_ref) == expected.provider_ref.as_ref()
            })
        {
            return Err(conflict());
        }
        let payment = repair(current, &expected)?;
        let mut audit = self
            .payment_cancellation_audit
            .lock()
            .map_err(storage_error)?;
        audit.push(PaymentCancellationAudit {
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
    /// Development-only inspection of explicit cancellation decisions.
    /// # Errors
    /// Returns a storage error if the audit lock was poisoned.
    pub fn payment_cancellation_audits(&self) -> BlueprintResult<Vec<PaymentCancellationAudit>> {
        Ok(self
            .payment_cancellation_audit
            .lock()
            .map_err(storage_error)?
            .clone())
    }
}
