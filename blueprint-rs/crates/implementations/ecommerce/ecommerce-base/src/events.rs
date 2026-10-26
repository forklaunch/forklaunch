//! Atomic, replay-safe inventory side effects for order transitions.
use crate::{BaseEcommerceService, store::EcommerceStore};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::{OrderStatus, events::OrderEvent};
use std::collections::BTreeMap;

/// Validates event identity and quantities, aggregating duplicate variants.
/// Native workers reject forged/non-transition events before changing stock.
pub(crate) fn deltas(event: &OrderEvent) -> BlueprintResult<BTreeMap<String, i64>> {
    if event.id.is_empty()
        || event.order_id.is_empty()
        || !event.from_status.can_transition_to(event.to_status)
        || event
            .items
            .iter()
            .any(|item| item.quantity < 1 || item.variant_id.is_empty())
    {
        return Err(invalid_event());
    }
    let sign = if event.to_status == OrderStatus::Paid {
        -1_i64
    } else {
        i64::from(
            event.to_status == OrderStatus::Cancelled
                && matches!(
                    event.from_status,
                    OrderStatus::Paid | OrderStatus::Fulfilled
                ),
        )
    };
    let mut result: BTreeMap<String, i64> = BTreeMap::new();
    if sign != 0 {
        for item in &event.items {
            let current = result.entry(item.variant_id.clone()).or_default();
            *current = current
                .checked_add(sign * item.quantity)
                .ok_or_else(invalid_event)?;
        }
    }
    Ok(result)
}
pub(crate) fn fingerprint(event: &OrderEvent) -> BlueprintResult<String> {
    // Queue delivery metadata does not change the logical event identity.
    serde_json::to_string(&(
        &event.order_id,
        event.from_status,
        event.to_status,
        &event.items,
    ))
    .map_err(|_| invalid_event())
}
pub(crate) fn invalid_event() -> BlueprintError {
    BlueprintError::new("ecommerce_invalid_event", "Invalid order inventory event")
}
pub(crate) fn conflicting_event() -> BlueprintError {
    BlueprintError::new(
        "ecommerce_event_conflict",
        "Order event identity already has a different payload",
    )
}
impl<S: EcommerceStore> BaseEcommerceService<S> {
    /// Applies every inventory delta and records the event in one transaction.
    /// Returns false for a previously applied identical event. Queue receipt loss
    /// or cancellation after commit can safely redeliver the original payload.
    /// # Errors
    /// Returns validation, stock/storage or conflicting event-identity errors.
    pub async fn apply_order_event(&self, event: &OrderEvent) -> BlueprintResult<bool> {
        self.store().apply_order_event(event.clone()).await
    }
}

#[cfg(feature = "worker")]
pub struct OrderInventoryHandler<S> {
    store: std::sync::Arc<S>,
    queue: String,
}
#[cfg(feature = "worker")]
impl<S> OrderInventoryHandler<S> {
    #[must_use]
    pub fn new(store: std::sync::Arc<S>, queue: impl Into<String>) -> Self {
        Self {
            store,
            queue: queue.into(),
        }
    }
}
#[cfg(feature = "worker")]
impl<S: EcommerceStore> forklaunch_worker_database::runner::JobHandler
    for OrderInventoryHandler<S>
{
    async fn process(&self, job: forklaunch_worker_interface::Job) -> BlueprintResult<()> {
        let event: OrderEvent =
            serde_json::from_str(&job.payload_json).map_err(|_| invalid_event())?;
        if job.queue != self.queue || job.id != event.id {
            return Err(invalid_event());
        }
        self.store.apply_order_event(event).await?;
        Ok(())
    }
}
