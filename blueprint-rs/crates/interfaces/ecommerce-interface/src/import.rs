//! Normalized bulk catalog imports; IDs are assigned by the owning service.
use crate::{ProductImage, ProductOption, optional};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportProduct {
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
    pub variants: Vec<ImportVariant>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportVariant {
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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "i64")]
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub initial_stock: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogImport {
    pub products: Vec<ImportProduct>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogImportResult {
    #[schemars(range(min = 0, max = 9_007_199_254_740_991i64))]
    pub products_imported: u64,
    #[schemars(range(min = 0, max = 9_007_199_254_740_991i64))]
    pub variants_imported: u64,
}
