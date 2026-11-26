//! Durable order-transition event payload consumed by inventory workers.
use crate::{OrderItemDto, OrderStatus};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderEvent {
    pub id: String,
    pub order_id: String,
    pub from_status: OrderStatus,
    pub to_status: OrderStatus,
    pub items: Vec<OrderItemDto>,
    #[serde(default)]
    pub processed: bool,
    #[serde(default)]
    pub retry_count: u32,
}
