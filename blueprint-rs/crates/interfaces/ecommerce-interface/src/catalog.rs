//! Storefront catalog views. Missing inventory remains absent, rather than zero.
use crate::{ProductDto, VariantDto, optional};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct CatalogQuery {
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
    pub limit: Option<i64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "i64")]
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub offset: Option<i64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogVariant {
    #[serde(flatten)]
    pub variant: VariantDto,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "i64")]
    #[schemars(range(min = -9_007_199_254_740_991i64, max = 9_007_199_254_740_991i64))]
    pub stock: Option<i64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogProduct {
    #[serde(flatten)]
    pub product: ProductDto,
    pub variants: Vec<CatalogVariant>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogPage {
    pub products: Vec<CatalogProduct>,
    #[schemars(range(min = 0, max = 9_007_199_254_740_991i64))]
    pub total: u64,
    pub limit: u32,
    #[schemars(range(min = 0, max = 9_007_199_254_740_991i64))]
    pub offset: u64,
}
