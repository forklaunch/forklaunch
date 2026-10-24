//! Read-only operator review of retained payment references on cancelled orders.
//! A local succeeded state can be an interrupted claim, not proof of capture.
use crate::{
    BaseEcommerceService,
    store::{EcommerceStore, Record, RecordKind, not_found, storage_error},
};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::{OrderStatus, PaymentDto, PaymentStatus};
use serde::{Deserialize, Serialize};
use std::future::Future;

/// Decisions deliberately stop short of authorizing refund, reference reuse,
/// order resurrection or delivery. Persisted evidence is historical, not a new
/// provider lookup; captured funds still need an operator's provider review.
/// Audits bind payment ID/reference/amount/currency, not a historical order
/// version or merchant account. An identical low-level delete/recreate cannot
/// be distinguished by this schema and never authorizes automatic repair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentReviewState {
    NoProviderReference,
    UnverifiedPending,
    UnverifiedClaimOrCapture,
    UnverifiedFailure,
    CapturedNeedsOperatorReview,
    CancellationRecordedKeepReference,
    ContradictoryEvidence,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentReferenceReview {
    pub payment_id: String,
    pub provider_ref: Option<String>,
    pub local_status: PaymentStatus,
    pub amount_cents: i64,
    pub currency: String,
    pub state: PaymentReviewState,
}
/// One atomic local snapshot, containing no customer/address/provider payloads.
/// Later pages are independent snapshots: concurrent insertions before a cursor
/// require a fresh scan. A report is informational, never a mutation capability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelledOrderPaymentReview {
    pub order_id: String,
    pub payments: Vec<PaymentReferenceReview>,
    pub next_cursor: Option<String>,
}
/// Tenant-bound snapshot access without decrypting order PII.
pub trait CancelledOrderPaymentReviewStore: EcommerceStore {
    /// # Errors
    /// Rejects noncancelled/missing orders, invalid limits/cursors and storage failures.
    fn review_cancelled_order_payments(
        &self,
        order_id: &str,
        after: Option<&str>,
        limit: usize,
    ) -> impl Future<Output = BlueprintResult<CancelledOrderPaymentReview>> + Send;
}
impl<S: CancelledOrderPaymentReviewStore> BaseEcommerceService<S> {
    /// Returns a read-only operator report. `limit` is 1..=100; pass `next_cursor`
    /// as `after` for the next page. Keep every provider reference until a separate
    /// authenticated provider workflow has resolved its monetary effects.
    /// # Errors
    /// Rejects noncancelled/missing orders, invalid input and storage failures.
    pub async fn review_cancelled_order_payments(
        &self,
        order_id: &str,
        after: Option<&str>,
        limit: usize,
    ) -> BlueprintResult<CancelledOrderPaymentReview> {
        self.store
            .review_cancelled_order_payments(order_id, after, limit)
            .await
    }
}
pub(crate) fn validate(order_id: &str, after: Option<&str>, limit: usize) -> BlueprintResult<()> {
    if !crate::reconciliation::valid_id(order_id)
        || after.is_some_and(|id| !crate::reconciliation::valid_id(id))
        || !(1..=100).contains(&limit)
    {
        return Err(BlueprintError::new(
            "ecommerce_invalid_payment_review",
            "Invalid order, cursor or page limit",
        ));
    }
    Ok(())
}
pub(crate) fn cancelled(status: OrderStatus) -> BlueprintResult<()> {
    if status != OrderStatus::Cancelled {
        return Err(BlueprintError::new(
            "ecommerce_order_not_cancelled",
            "Payment reference review requires a cancelled order",
        ));
    }
    Ok(())
}
pub(crate) fn review(
    payment: PaymentDto,
    captured: bool,
    canceled: bool,
    inconsistent: bool,
) -> PaymentReferenceReview {
    let state = if inconsistent
        || (captured && canceled)
        || (captured && payment.status != PaymentStatus::Succeeded)
        || (canceled && payment.status != PaymentStatus::Failed)
    {
        PaymentReviewState::ContradictoryEvidence
    } else if payment.provider_ref.as_deref().is_none_or(str::is_empty) {
        PaymentReviewState::NoProviderReference
    } else if captured {
        PaymentReviewState::CapturedNeedsOperatorReview
    } else if canceled {
        PaymentReviewState::CancellationRecordedKeepReference
    } else {
        match payment.status {
            PaymentStatus::Pending => PaymentReviewState::UnverifiedPending,
            PaymentStatus::Succeeded => PaymentReviewState::UnverifiedClaimOrCapture,
            PaymentStatus::Failed => PaymentReviewState::UnverifiedFailure,
        }
    };
    PaymentReferenceReview {
        payment_id: payment.id,
        provider_ref: payment.provider_ref,
        local_status: payment.status,
        amount_cents: payment.amount_cents,
        currency: payment.currency,
        state,
    }
}
pub(crate) fn page(
    order_id: &str,
    mut payments: Vec<PaymentReferenceReview>,
    limit: usize,
) -> CancelledOrderPaymentReview {
    let next_cursor = if payments.len() > limit {
        payments.truncate(limit);
        payments.last().map(|payment| payment.payment_id.clone())
    } else {
        None
    };
    CancelledOrderPaymentReview {
        order_id: order_id.into(),
        payments,
        next_cursor,
    }
}
impl CancelledOrderPaymentReviewStore for crate::store::InMemoryEcommerceStore {
    async fn review_cancelled_order_payments(
        &self,
        order_id: &str,
        after: Option<&str>,
        limit: usize,
    ) -> BlueprintResult<CancelledOrderPaymentReview> {
        validate(order_id, after, limit)?;
        let records = self.records.lock().map_err(storage_error)?;
        let Some(Record::Order(order)) = records.get(&(RecordKind::Order, order_id.into())) else {
            return Err(not_found(RecordKind::Order));
        };
        cancelled(order.status)?;
        // All payment/capture/cancellation writers acquire records first.
        let captures = self
            .payment_reconciliation_audit
            .lock()
            .map_err(storage_error)?;
        let cancellations = self
            .payment_cancellation_audit
            .lock()
            .map_err(storage_error)?;
        let payments = records
            .values()
            .filter_map(|record| {
                let Record::Payment(payment) = record else {
                    return None;
                };
                if payment.order_id != order_id
                    || after.is_some_and(|cursor| payment.id.as_str() <= cursor)
                {
                    return None;
                }
                let matching = |id: &str, reference: &str| {
                    id == payment.id && payment.provider_ref.as_deref() == Some(reference)
                };
                let capture_rows: Vec<_> = captures
                    .iter()
                    .filter(|audit| matching(&audit.payment_id, &audit.evidence.provider_ref))
                    .collect();
                let cancel_rows: Vec<_> = cancellations
                    .iter()
                    .filter(|audit| matching(&audit.payment_id, &audit.evidence.provider_ref))
                    .collect();
                let mut providers = std::collections::BTreeSet::new();
                let inconsistent = capture_rows.iter().any(|audit| {
                    providers.insert(audit.evidence.provider.as_str());
                    audit.evidence.amount_cents != payment.amount_cents
                        || !audit
                            .evidence
                            .currency
                            .eq_ignore_ascii_case(&payment.currency)
                }) || cancel_rows.iter().any(|audit| {
                    providers.insert(audit.evidence.provider.as_str());
                    audit.evidence.amount_cents != payment.amount_cents
                        || !audit
                            .evidence
                            .currency
                            .eq_ignore_ascii_case(&payment.currency)
                });
                Some(review(
                    payment.clone(),
                    !capture_rows.is_empty(),
                    !cancel_rows.is_empty(),
                    inconsistent || providers.len() > 1,
                ))
            })
            .take(limit + 1)
            .collect();
        Ok(page(order_id, payments, limit))
    }
}
