//! Read-only guards for returning an existing provider credential. These checks
//! cannot revoke previously issued secrets or eliminate the post-check race.
use crate::store::{EcommerceStore, InMemoryEcommerceStore, Record, RecordKind, storage_error};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::{OrderStatus, PaymentDto};
use std::future::Future;
/// Tenant-bound current payment/order relation without order address reads.
pub trait PaymentResumeStore: EcommerceStore {
    /// # Errors
    /// Rejects changed/missing identities and missing or cancelled linked orders.
    fn validate_payment_resume(
        &self,
        expected: &PaymentDto,
    ) -> impl Future<Output = BlueprintResult<()>> + Send;
}
pub(crate) fn conflict() -> BlueprintError {
    BlueprintError::new(
        "ecommerce_conflict",
        "Payment cannot be resumed for this order",
    )
}
impl PaymentResumeStore for InMemoryEcommerceStore {
    async fn validate_payment_resume(&self, expected: &PaymentDto) -> BlueprintResult<()> {
        let records = self.records.lock().map_err(storage_error)?;
        let Some(Record::Payment(payment)) =
            records.get(&(RecordKind::Payment, expected.id.clone()))
        else {
            return Err(conflict());
        };
        if payment.id != expected.id
            || payment.order_id != expected.order_id
            || payment.provider_ref != expected.provider_ref
        {
            return Err(conflict());
        }
        let Some(Record::Order(order)) =
            records.get(&(RecordKind::Order, payment.order_id.clone()))
        else {
            return Err(conflict());
        };
        if order.id != payment.order_id || order.status == OrderStatus::Cancelled {
            return Err(conflict());
        }
        Ok(())
    }
}
