//! Tenant-bound lookups used to resume pending checkout orders and payments.
use crate::{
    BaseEcommerceService,
    store::{EcommerceStore, Field, Query, RecordKind},
};
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::{OrderDto, OrderStatus, PaymentDto, PaymentStatus};
impl<S: EcommerceStore> BaseEcommerceService<S> {
    /// # Errors
    /// Propagates storage and audited plaintext-read errors.
    pub async fn pending_order_for_cart(&self, cart_id: &str) -> BlueprintResult<Option<OrderDto>> {
        let rows: Vec<OrderDto> = self
            .many(Query {
                equals: Some((Field::CartId, cart_id.to_owned())),
                ..Query::all(RecordKind::Order)
            })
            .await?;
        Ok(rows
            .into_iter()
            .find(|order| order.status == OrderStatus::Pending))
    }
    /// # Errors
    /// Propagates storage errors. Providerless and spent payments cannot be resumed.
    pub async fn pending_payment_for_order(
        &self,
        order_id: &str,
    ) -> BlueprintResult<Option<PaymentDto>> {
        let rows: Vec<PaymentDto> = self
            .many(Query {
                equals: Some((Field::OrderId, order_id.to_owned())),
                ..Query::all(RecordKind::Payment)
            })
            .await?;
        Ok(rows.into_iter().find(|payment| {
            payment.status == PaymentStatus::Pending
                && payment
                    .provider_ref
                    .as_ref()
                    .is_some_and(|value| !value.is_empty())
        }))
    }
}
