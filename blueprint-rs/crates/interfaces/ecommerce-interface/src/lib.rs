//! Ecommerce DTOs and asynchronous service interfaces shared with TypeScript.
#![forbid(unsafe_code)]
pub mod catalog;
pub mod contracts;
pub mod events;
pub mod import;
pub mod module;
pub mod pricing;
pub mod webhook;
use forklaunch_core::domain::BlueprintResult;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, future::Future};

// Quantities, stock and money cross the JSON boundary as numbers in both SDKs.
// Their schemas retain integer semantics and the exact integer range shared by
// Rust and JavaScript. Stock adjustments remain signed for inventory decrements.

// Optional input fields distinguish omission from explicit JSON null, matching
// the upstream non-nullable optional schemas.
fn optional<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IdDto {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IdsDto {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductOption {
    pub name: String,
    pub is_pack_quantity: bool,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductImage {
    pub src: String,
    pub position: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CartItemDto {
    pub variant_id: String,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub quantity: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderItemDto {
    pub variant_id: String,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub quantity: i64,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub unit_price_cents: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShippingAddressDto {
    pub name: String,
    pub line1: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub line2: Option<String>,
    pub city: String,
    pub state: String,
    pub postal_code: String,
    pub country: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaxLineDto {
    pub jurisdiction: String,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub tax_cents: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum OrderStatus {
    Pending,
    Paid,
    Fulfilled,
    Shipped,
    Delivered,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PaymentStatus {
    Pending,
    Succeeded,
    Failed,
}

impl OrderStatus {
    #[must_use]
    pub const fn can_transition_to(self, to: Self) -> bool {
        matches!(
            (self, to),
            (Self::Pending, Self::Paid | Self::Cancelled)
                | (Self::Paid, Self::Fulfilled | Self::Cancelled)
                | (Self::Fulfilled, Self::Shipped | Self::Cancelled)
                | (Self::Shipped, Self::Delivered)
        )
    }
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Paid => "paid",
            Self::Fulfilled => "fulfilled",
            Self::Shipped => "shipped",
            Self::Delivered => "delivered",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateProductDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub id: Option<String>,
    pub external_id: String,
    pub handle: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub source_url: Option<String>,
    pub title: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub description_html: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub vendor: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub product_type: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Vec<String>")]
    pub tags: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Vec<ProductOption>")]
    pub options: Option<Vec<ProductOption>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Vec<ProductImage>")]
    pub images: Option<Vec<ProductImage>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductDto {
    pub id: String,
    pub external_id: String,
    pub handle: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub source_url: Option<String>,
    pub title: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub description_html: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub vendor: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub product_type: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Vec<String>")]
    pub tags: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Vec<ProductOption>")]
    pub options: Option<Vec<ProductOption>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Vec<ProductImage>")]
    pub images: Option<Vec<ProductImage>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub created_at: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProductDto {
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub external_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub handle: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub source_url: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub title: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub description_html: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub vendor: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub product_type: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Vec<String>")]
    pub tags: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Vec<ProductOption>")]
    pub options: Option<Vec<ProductOption>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Vec<ProductImage>")]
    pub images: Option<Vec<ProductImage>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateVariantDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub id: Option<String>,
    pub product_id: String,
    pub external_id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub sku: Option<String>,
    pub title: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "BTreeMap<String, String>")]
    pub option_values: Option<BTreeMap<String, String>>,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub price_cents: i64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "i64")]
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub compare_at_price_cents: Option<i64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "bool")]
    pub requires_shipping: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VariantDto {
    pub id: String,
    pub product_id: String,
    pub external_id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub sku: Option<String>,
    pub title: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "BTreeMap<String, String>")]
    pub option_values: Option<BTreeMap<String, String>>,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub price_cents: i64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "i64")]
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub compare_at_price_cents: Option<i64>,
    pub requires_shipping: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub created_at: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateVariantDto {
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub product_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub external_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub sku: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub title: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "BTreeMap<String, String>")]
    pub option_values: Option<BTreeMap<String, String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "i64")]
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub price_cents: Option<i64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "i64")]
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub compare_at_price_cents: Option<i64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "bool")]
    pub requires_shipping: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateInventoryDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub id: Option<String>,
    pub variant_id: String,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub stock: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryDto {
    pub id: String,
    pub variant_id: String,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub stock: i64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub created_at: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInventoryDto {
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub variant_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "i64")]
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub stock: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreateCartDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub customer_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CartDto {
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub customer_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub created_at: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub updated_at: Option<String>,
    pub status: String,
    pub items: Vec<CartItemDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCartDto {
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub customer_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateOrderDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub customer_id: Option<String>,
    pub items: Vec<OrderItemDto>,
    pub shipping_address: ShippingAddressDto,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub subtotal_cents: i64,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub discount_cents: i64,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub tax_cents: i64,
    pub tax_breakdown: Vec<TaxLineDto>,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub shipping_cents: i64,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub gift_card_cents: i64,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub total_cents: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderDto {
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub customer_id: Option<String>,
    pub items: Vec<OrderItemDto>,
    pub shipping_address: ShippingAddressDto,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub subtotal_cents: i64,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub discount_cents: i64,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub tax_cents: i64,
    pub tax_breakdown: Vec<TaxLineDto>,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub shipping_cents: i64,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub gift_card_cents: i64,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub total_cents: i64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub created_at: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub updated_at: Option<String>,
    pub status: OrderStatus,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub cart_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatePaymentDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub id: Option<String>,
    pub order_id: String,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub amount_cents: i64,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentDto {
    pub id: String,
    pub order_id: String,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub amount_cents: i64,
    pub currency: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub created_at: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub updated_at: Option<String>,
    pub status: PaymentStatus,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub provider_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOrderDto {
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "OrderStatus")]
    pub status: Option<OrderStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProductSearchDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Vec<String>")]
    pub ids: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub title: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "i64")]
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub min_price_cents: Option<i64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "i64")]
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub max_price_cents: Option<i64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "bool")]
    pub in_stock: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub option_name: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub option_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AddCartItemDto {
    pub cart_id: String,
    pub variant_id: String,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub quantity: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemoveCartItemDto {
    pub cart_id: String,
    pub variant_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdjustStockDto {
    pub variant_id: String,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub delta: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockCheckDto {
    pub variant_id: String,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub requested: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockCheckResultDto {
    pub variant_id: String,
    pub available: bool,
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub stock: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TransitionOrderDto {
    pub id: String,
    pub to: OrderStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmPaymentDto {
    pub provider_ref: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FailPaymentDto {
    pub provider_ref: String,
}

pub trait ProductService: Send + Sync {
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn create_product(
        &self,
        request: CreateProductDto,
    ) -> impl Future<Output = BlueprintResult<ProductDto>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn get_product(&self, id: &str) -> impl Future<Output = BlueprintResult<ProductDto>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn get_product_by_handle(
        &self,
        handle: &str,
    ) -> impl Future<Output = BlueprintResult<ProductDto>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn get_product_by_external_id(
        &self,
        external_id: &str,
    ) -> impl Future<Output = BlueprintResult<ProductDto>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn update_product(
        &self,
        request: UpdateProductDto,
    ) -> impl Future<Output = BlueprintResult<ProductDto>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn delete_product(&self, id: &str) -> impl Future<Output = BlueprintResult<()>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn list_products(
        &self,
        search: Option<ProductSearchDto>,
    ) -> impl Future<Output = BlueprintResult<Vec<ProductDto>>> + Send;
}

pub trait VariantService: Send + Sync {
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn create_variant(
        &self,
        request: CreateVariantDto,
    ) -> impl Future<Output = BlueprintResult<VariantDto>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn get_variant(&self, id: &str) -> impl Future<Output = BlueprintResult<VariantDto>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn get_variant_by_external_id(
        &self,
        external_id: &str,
    ) -> impl Future<Output = BlueprintResult<VariantDto>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn update_variant(
        &self,
        request: UpdateVariantDto,
    ) -> impl Future<Output = BlueprintResult<VariantDto>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn delete_variant(&self, id: &str) -> impl Future<Output = BlueprintResult<()>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn list_variants(
        &self,
        ids: Option<Vec<String>>,
    ) -> impl Future<Output = BlueprintResult<Vec<VariantDto>>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn list_variants_by_product(
        &self,
        product_id: &str,
    ) -> impl Future<Output = BlueprintResult<Vec<VariantDto>>> + Send;
}

pub trait InventoryService: Send + Sync {
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn create_inventory(
        &self,
        request: CreateInventoryDto,
    ) -> impl Future<Output = BlueprintResult<InventoryDto>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn get_inventory(
        &self,
        variant_id: &str,
    ) -> impl Future<Output = BlueprintResult<InventoryDto>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn adjust_stock(
        &self,
        request: AdjustStockDto,
    ) -> impl Future<Output = BlueprintResult<InventoryDto>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn check_stock(
        &self,
        request: StockCheckDto,
    ) -> impl Future<Output = BlueprintResult<StockCheckResultDto>> + Send;
}

pub trait CartService: Send + Sync {
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn create_cart(
        &self,
        request: CreateCartDto,
    ) -> impl Future<Output = BlueprintResult<CartDto>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn get_cart(&self, id: &str) -> impl Future<Output = BlueprintResult<CartDto>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn add_item(
        &self,
        request: AddCartItemDto,
    ) -> impl Future<Output = BlueprintResult<CartDto>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn remove_item(
        &self,
        request: RemoveCartItemDto,
    ) -> impl Future<Output = BlueprintResult<CartDto>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn clear_cart(&self, id: &str) -> impl Future<Output = BlueprintResult<()>> + Send;
}

pub trait OrderService: Send + Sync {
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn create_order(
        &self,
        request: CreateOrderDto,
    ) -> impl Future<Output = BlueprintResult<OrderDto>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn get_order(&self, id: &str) -> impl Future<Output = BlueprintResult<OrderDto>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn list_orders(
        &self,
        ids: Option<Vec<String>>,
    ) -> impl Future<Output = BlueprintResult<Vec<OrderDto>>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn transition_order(
        &self,
        request: TransitionOrderDto,
    ) -> impl Future<Output = BlueprintResult<OrderDto>> + Send;
}

pub trait PaymentService: Send + Sync {
    /// Provider create responses may include ephemeral checkout credentials.
    /// Persistent reads continue to return the base payment DTO.
    type CreatedPayment: Into<PaymentDto> + serde::Serialize + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn create_payment(
        &self,
        request: CreatePaymentDto,
    ) -> impl Future<Output = BlueprintResult<Self::CreatedPayment>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn get_payment(&self, id: &str) -> impl Future<Output = BlueprintResult<PaymentDto>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn confirm_payment(
        &self,
        request: ConfirmPaymentDto,
    ) -> impl Future<Output = BlueprintResult<PaymentDto>> + Send;
    /// # Errors
    /// Returns typed validation, missing-record, concurrency or provider/storage failures.
    fn fail_payment(
        &self,
        request: FailPaymentDto,
    ) -> impl Future<Output = BlueprintResult<PaymentDto>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_ecommerce_integers_preserve_signed_numbers_and_exact_boundaries() {
        const SAFE: i64 = 9_007_199_254_740_991;
        let schema = serde_json::to_value(schemars::schema_for!(AdjustStockDto)).unwrap();
        let delta = &schema["properties"]["delta"];
        assert_eq!(delta["type"], "integer");
        let validate =
            forklaunch_core::contract::schema_validation::compile_checked(delta).unwrap();
        for value in [-SAFE, -42, 0, 42, SAFE] {
            validate(&serde_json::json!(value)).unwrap();
        }
        for invalid in [
            serde_json::json!(-SAFE - 1),
            serde_json::json!(SAFE + 1),
            serde_json::json!(1.5),
            serde_json::json!("42"),
        ] {
            assert!(validate(&invalid).is_err());
        }
        let wire = serde_json::to_value(AdjustStockDto {
            variant_id: "variant-1".into(),
            delta: -42,
        })
        .unwrap();
        assert_eq!(wire["delta"], serde_json::json!(-42));

        let schema =
            serde_json::to_value(schemars::schema_for!(import::CatalogImportResult)).unwrap();
        let count = &schema["properties"]["productsImported"];
        let validate =
            forklaunch_core::contract::schema_validation::compile_checked(count).unwrap();
        validate(&serde_json::json!(SAFE)).unwrap();
        assert!(validate(&serde_json::json!(-1)).is_err());
        assert!(validate(&serde_json::json!(SAFE + 1)).is_err());

        let schema = serde_json::to_value(schemars::schema_for!(pricing::TaxCalculation)).unwrap();
        let amounts = &schema["properties"]["lineItemCents"];
        let validate =
            forklaunch_core::contract::schema_validation::compile_checked(amounts).unwrap();
        validate(&serde_json::json!([0, 42, SAFE])).unwrap();
        assert!(validate(&serde_json::json!([SAFE + 1])).is_err());
        assert!(validate(&serde_json::json!([1.5])).is_err());
    }
}
