//! Native durable customer records. Provider customer creation is a separate concern.
use chrono::{DateTime, Utc};
use forklaunch_core::domain::BlueprintResult;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::future::Future;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateCustomerDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub email: String,
    pub name: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateCustomerDto {
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub email: Option<String>,
    /// Omitted leaves the name unchanged; explicit null clears it.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    #[allow(clippy::option_option)]
    // Distinguishes omitted, explicit null, and a replacement name.
    pub name: Option<Option<String>>,
}
fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomerDto {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
/// Tenant-bound durable customer CRUD; provider calls are deliberately separate.
pub trait CustomerService: Send + Sync {
    fn create_customer(
        &self,
        request: CreateCustomerDto,
    ) -> impl Future<Output = BlueprintResult<CustomerDto>> + Send;
    fn get_customer(&self, id: &str) -> impl Future<Output = BlueprintResult<CustomerDto>> + Send;
    fn update_customer(
        &self,
        request: UpdateCustomerDto,
    ) -> impl Future<Output = BlueprintResult<CustomerDto>> + Send;
    fn delete_customer(&self, id: &str) -> impl Future<Output = BlueprintResult<()>> + Send;
    fn list_customers(
        &self,
        ids: Option<&[String]>,
    ) -> impl Future<Output = BlueprintResult<Vec<CustomerDto>>> + Send;
}
