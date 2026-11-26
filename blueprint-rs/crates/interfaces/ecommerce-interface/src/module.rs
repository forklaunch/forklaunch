//! HTTP module request/response DTOs distinct from persistence service inputs.
use crate::{OrderStatus, PaymentDto, optional};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PaymentProvider {
    Stripe,
    Paypal,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateProviderPayment {
    pub order_id: String,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub amount_cents: i64,
    pub currency: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "PaymentProvider")]
    pub provider: Option<PaymentProvider>,
}
/// Creation can include an ephemeral Stripe secret; persistent reads cannot.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatedPayment {
    #[serde(flatten)]
    pub payment: PaymentDto,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub client_secret: Option<String>,
}
impl std::fmt::Debug for CreatedPayment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreatedPayment")
            .field("payment", &self.payment)
            .finish_non_exhaustive()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OrderTransition {
    pub to: OrderStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Checkout {
    pub cart_id: String,
    pub shipping_address: crate::ShippingAddressDto,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "PaymentProvider")]
    pub provider: Option<PaymentProvider>,
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutResult {
    pub order: crate::OrderDto,
    pub payment: PaymentDto,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub client_secret: Option<String>,
}
impl std::fmt::Debug for CheckoutResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckoutResult")
            .field("order_id", &self.order.id)
            .field("payment_id", &self.payment.id)
            .finish_non_exhaustive()
    }
}
