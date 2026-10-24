//! Typed asynchronous billing services corresponding to the five TypeScript service interfaces.
//! Enum strings are application-defined, as in the generic TypeScript interfaces.
use chrono::{DateTime, Utc};
use forklaunch_core::domain::BlueprintResult;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;

fn optional<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatePlanDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub id: Option<String>,
    pub name: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub description: Option<String>,
    pub price: f64,
    pub cadence: String,
    pub currency: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Vec<String>")]
    pub features: Option<Vec<String>>,
    pub external_id: String,
    pub billing_provider: String,
    pub active: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePlanDto {
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub name: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub description: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "f64")]
    pub price: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub cadence: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub currency: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Vec<String>")]
    pub features: Option<Vec<String>>,
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
    pub billing_provider: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "bool")]
    pub active: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlanDto {
    pub id: String,
    pub name: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub description: Option<String>,
    pub price: f64,
    pub cadence: String,
    pub currency: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Vec<String>")]
    pub features: Option<Vec<String>>,
    pub external_id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub billing_provider: Option<String>,
    pub active: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSubscriptionDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub id: Option<String>,
    pub party_id: String,
    pub party_type: String,
    pub product_id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub description: Option<String>,
    pub active: bool,
    pub external_id: String,
    pub start_date: DateTime<Utc>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub end_date: Option<DateTime<Utc>>,
    pub status: String,
    pub billing_provider: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSubscriptionDto {
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub party_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub party_type: Option<String>,
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
    pub description: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "bool")]
    pub active: Option<bool>,
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
    #[schemars(with = "DateTime<Utc>")]
    pub start_date: Option<DateTime<Utc>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub end_date: Option<DateTime<Utc>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub status: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub billing_provider: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionDto {
    pub id: String,
    pub party_id: String,
    pub party_type: String,
    pub product_id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub description: Option<String>,
    pub active: bool,
    pub external_id: String,
    pub start_date: DateTime<Utc>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub end_date: Option<DateTime<Utc>>,
    pub status: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub billing_provider: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateCheckoutSessionDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub id: Option<String>,
    pub customer_id: String,
    pub payment_methods: Vec<String>,
    pub currency: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub uri: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub success_redirect_uri: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub cancel_redirect_uri: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub status: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckoutSessionDto {
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
    #[schemars(with = "Vec<String>")]
    pub payment_methods: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub currency: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub uri: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub success_redirect_uri: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub cancel_redirect_uri: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub status: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutSessionDto {
    pub id: String,
    pub customer_id: String,
    pub payment_methods: Vec<String>,
    pub currency: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub uri: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub success_redirect_uri: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub cancel_redirect_uri: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub status: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatePaymentLinkDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub id: Option<String>,
    pub amount: f64,
    pub payment_methods: Vec<String>,
    pub currency: String,
    pub status: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePaymentLinkDto {
    pub id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "f64")]
    pub amount: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Vec<String>")]
    pub payment_methods: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub currency: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub status: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentLinkDto {
    pub id: String,
    pub amount: f64,
    pub payment_methods: Vec<String>,
    pub currency: String,
    pub status: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateBillingPortalDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub id: Option<String>,
    pub customer_id: String,
    pub expires_at: DateTime<Utc>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub uri: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateBillingPortalDto {
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
    #[schemars(with = "DateTime<Utc>")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub uri: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BillingPortalDto {
    pub id: String,
    pub customer_id: String,
    pub expires_at: DateTime<Utc>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "String")]
    pub uri: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "Value")]
    pub provider_fields: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "optional"
    )]
    #[schemars(with = "DateTime<Utc>")]
    pub updated_at: Option<DateTime<Utc>>,
}

pub trait PlanService: Send + Sync {
    fn create_plan(
        &self,
        request: CreatePlanDto,
    ) -> impl Future<Output = BlueprintResult<PlanDto>> + Send;
    fn get_plan(&self, id: &str) -> impl Future<Output = BlueprintResult<PlanDto>> + Send;
    fn update_plan(
        &self,
        request: UpdatePlanDto,
    ) -> impl Future<Output = BlueprintResult<PlanDto>> + Send;
    fn delete_plan(&self, id: &str) -> impl Future<Output = BlueprintResult<()>> + Send;
    fn list_plans(
        &self,
        ids: Option<&[String]>,
    ) -> impl Future<Output = BlueprintResult<Vec<PlanDto>>> + Send;
}

pub trait SubscriptionService: Send + Sync {
    fn create_subscription(
        &self,
        request: CreateSubscriptionDto,
    ) -> impl Future<Output = BlueprintResult<SubscriptionDto>> + Send;
    fn get_subscription(
        &self,
        id: &str,
    ) -> impl Future<Output = BlueprintResult<SubscriptionDto>> + Send;
    fn get_user_subscription(
        &self,
        id: &str,
    ) -> impl Future<Output = BlueprintResult<SubscriptionDto>> + Send;
    fn get_organization_subscription(
        &self,
        id: &str,
    ) -> impl Future<Output = BlueprintResult<SubscriptionDto>> + Send;
    fn find_subscription_id_by_external_id(
        &self,
        external_id: &str,
    ) -> impl Future<Output = BlueprintResult<Option<String>>> + Send;
    fn update_subscription(
        &self,
        request: UpdateSubscriptionDto,
    ) -> impl Future<Output = BlueprintResult<SubscriptionDto>> + Send;
    fn delete_subscription(&self, id: &str) -> impl Future<Output = BlueprintResult<()>> + Send;
    fn list_subscriptions(
        &self,
        ids: Option<&[String]>,
    ) -> impl Future<Output = BlueprintResult<Vec<SubscriptionDto>>> + Send;
    fn cancel_subscription(&self, id: &str) -> impl Future<Output = BlueprintResult<()>> + Send;
    fn resume_subscription(&self, id: &str) -> impl Future<Output = BlueprintResult<()>> + Send;
}

pub trait CheckoutSessionService: Send + Sync {
    fn create_checkout_session(
        &self,
        request: CreateCheckoutSessionDto,
    ) -> impl Future<Output = BlueprintResult<CheckoutSessionDto>> + Send;
    fn get_checkout_session(
        &self,
        id: &str,
    ) -> impl Future<Output = BlueprintResult<CheckoutSessionDto>> + Send;
    fn expire_checkout_session(&self, id: &str)
    -> impl Future<Output = BlueprintResult<()>> + Send;
    fn handle_checkout_success(&self, id: &str)
    -> impl Future<Output = BlueprintResult<()>> + Send;
    fn handle_checkout_failure(&self, id: &str)
    -> impl Future<Output = BlueprintResult<()>> + Send;
}

pub trait PaymentLinkService: Send + Sync {
    fn create_payment_link(
        &self,
        request: CreatePaymentLinkDto,
    ) -> impl Future<Output = BlueprintResult<PaymentLinkDto>> + Send;
    fn update_payment_link(
        &self,
        request: UpdatePaymentLinkDto,
    ) -> impl Future<Output = BlueprintResult<PaymentLinkDto>> + Send;
    fn get_payment_link(
        &self,
        id: &str,
    ) -> impl Future<Output = BlueprintResult<PaymentLinkDto>> + Send;
    fn expire_payment_link(&self, id: &str) -> impl Future<Output = BlueprintResult<()>> + Send;
    fn handle_payment_success(&self, id: &str) -> impl Future<Output = BlueprintResult<()>> + Send;
    fn handle_payment_failure(&self, id: &str) -> impl Future<Output = BlueprintResult<()>> + Send;
    fn list_payment_links(
        &self,
        ids: Option<&[String]>,
    ) -> impl Future<Output = BlueprintResult<Vec<PaymentLinkDto>>> + Send;
}

pub trait BillingPortalService: Send + Sync {
    fn create_billing_portal_session(
        &self,
        request: CreateBillingPortalDto,
    ) -> impl Future<Output = BlueprintResult<BillingPortalDto>> + Send;
    fn get_billing_portal_session(
        &self,
        id: &str,
    ) -> impl Future<Output = BlueprintResult<BillingPortalDto>> + Send;
    fn update_billing_portal_session(
        &self,
        request: UpdateBillingPortalDto,
    ) -> impl Future<Output = BlueprintResult<BillingPortalDto>> + Send;
    fn expire_billing_portal_session(
        &self,
        id: &str,
    ) -> impl Future<Output = BlueprintResult<()>> + Send;
}
