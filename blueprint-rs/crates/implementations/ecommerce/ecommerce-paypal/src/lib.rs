//! `PayPal` Orders v2 provider with atomic confirmation claims in the shared
//! ecommerce store. Webhook signatures are verified by `PayPal`'s HTTPS API.
#![forbid(unsafe_code)]
#[cfg(feature = "postgres")]
pub mod bound_reads;
pub mod context;
pub mod reconciliation;
#[cfg(feature = "postgres")]
pub mod scoped_cancellation;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_core::http::provider_http::provider_status_error;
use forklaunch_ecommerce_base::{BaseEcommerceService, store::EcommerceStore};
use forklaunch_ecommerce_interface::{
    ConfirmPaymentDto, CreatePaymentDto, FailPaymentDto, PaymentDto, PaymentService,
};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PaypalOrderStatus {
    Created,
    Saved,
    Approved,
    PayerActionRequired,
    Voided,
    Completed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaypalOrder {
    pub id: String,
    pub status: PaypalOrderStatus,
}
#[derive(Clone)]
pub struct PaypalClient {
    http: ProviderHttpClient,
    client_id: Arc<Zeroizing<String>>,
    client_secret: Arc<Zeroizing<String>>,
    generation: Arc<()>,
    scoped_environment: Option<context::PaypalEnvironment>,
}
impl std::fmt::Debug for PaypalClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaypalClient")
            .field("http", &self.http)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyWebhookSignature {
    pub transmission_id: String,
    pub transmission_time: String,
    pub transmission_sig: String,
    pub cert_url: String,
    pub auth_algo: String,
    pub webhook_id: String,
    pub webhook_event: Value,
}
impl PaypalClient {
    /// # Errors
    /// Rejects empty/control-containing credentials or an insecure origin.
    pub fn new(client_id: String, client_secret: String, base_url: &str) -> BlueprintResult<Self> {
        Self::with_transport(
            client_id,
            client_secret,
            ProviderHttpClient::new(base_url, Duration::from_secs(15), 1024 * 1024)?,
        )
    }
    /// # Errors
    /// Rejects empty/control-containing credentials.
    pub fn with_transport(
        client_id: String,
        client_secret: String,
        http: ProviderHttpClient,
    ) -> BlueprintResult<Self> {
        if [&client_id, &client_secret]
            .iter()
            .any(|value| value.is_empty() || value.bytes().any(|b| b.is_ascii_control()))
        {
            return Err(config_error());
        }
        Ok(Self {
            http,
            client_id: Arc::new(Zeroizing::new(client_id)),
            client_secret: Arc::new(Zeroizing::new(client_secret)),
            generation: Arc::new(()),
            scoped_environment: None,
        })
    }
    async fn access_token(&self) -> BlueprintResult<Zeroizing<String>> {
        let response = self
            .http
            .execute_json(
                self.http
                    .request(Method::POST, &["v1", "oauth2", "token"])?
                    .basic_auth(self.client_id.as_str(), Some(self.client_secret.as_str()))
                    .form(&[("grant_type", "client_credentials")]),
            )
            .await?;
        if !response.is_success() {
            return Err(provider_status_error("PayPal", "auth", response.status));
        }
        let token = response
            .body
            .get("access_token")
            .and_then(Value::as_str)
            .ok_or_else(shape_error)?;
        Ok(Zeroizing::new(token.to_owned()))
    }
    /// # Errors
    /// Returns sanitized provider/network/shape errors.
    pub async fn create_order(
        &self,
        amount_cents: i64,
        currency: &str,
        reference_id: &str,
    ) -> BlueprintResult<PaypalOrder> {
        let token = self.access_token().await?;
        // Integer formatting avoids binary floating-point errors at large values.
        let absolute = i128::from(amount_cents).abs();
        let amount = format!(
            "{}{whole}.{fraction:02}",
            if amount_cents < 0 { "-" } else { "" },
            whole = absolute / 100,
            fraction = absolute % 100
        );
        let request=self.http.request(Method::POST,&["v2","checkout","orders"])?.bearer_auth(token.as_str()).json(&json!({"intent":"CAPTURE","purchase_units":[{"reference_id":reference_id,"amount":{"currency_code":currency.to_uppercase(),"value":amount}}]}));
        let response = self.http.execute_json(request).await?;
        if !response.is_success() {
            return Err(provider_status_error(
                "PayPal",
                "create order",
                response.status,
            ));
        }
        serde_json::from_value(response.body).map_err(|_| shape_error())
    }
    /// # Errors
    /// Returns sanitized provider/network/shape errors; no automatic retries.
    pub async fn capture_order(&self, id: &str) -> BlueprintResult<PaypalOrder> {
        let token = self.access_token().await?;
        let response = self
            .http
            .execute_json(
                self.http
                    .request(Method::POST, &["v2", "checkout", "orders", id, "capture"])?
                    .bearer_auth(token.as_str())
                    .header("content-type", "application/json"),
            )
            .await?;
        if !response.is_success() {
            return Err(provider_status_error("PayPal", "capture", response.status));
        }
        let order: PaypalOrder =
            serde_json::from_value(response.body).map_err(|_| shape_error())?;
        if order.id != id {
            return Err(shape_error());
        }
        Ok(order)
    }
    /// The certificate URL is sent to `PayPal` as signed-event metadata. This
    /// process never fetches caller-provided certificate URLs itself.
    /// # Errors
    /// Returns authentication/network failures. Invalid verification replies and
    /// rejected signature requests return false without exposing response bodies.
    pub async fn verify_webhook_signature(
        &self,
        params: &VerifyWebhookSignature,
    ) -> BlueprintResult<bool> {
        let token = self.access_token().await?;
        let response=self.http.execute_json(self.http.request(Method::POST,&["v1","notifications","verify-webhook-signature"])?.bearer_auth(token.as_str()).json(&json!({"transmission_id":params.transmission_id,"transmission_time":params.transmission_time,"transmission_sig":params.transmission_sig,"cert_url":params.cert_url,"auth_algo":params.auth_algo,"webhook_id":params.webhook_id,"webhook_event":params.webhook_event}))).await?;
        Ok(response.is_success()
            && response
                .body
                .get("verification_status")
                .and_then(Value::as_str)
                == Some("SUCCESS"))
    }
}
/// Provider seam for owned fixtures/custom client integrations. Payment state
/// transitions remain in the persistent base service, independent of transport.
pub trait PaypalProvider: Send + Sync {
    fn create_order(
        &self,
        amount_cents: i64,
        currency: &str,
        reference_id: &str,
    ) -> impl std::future::Future<Output = BlueprintResult<PaypalOrder>> + Send;
    fn capture_order(
        &self,
        id: &str,
    ) -> impl std::future::Future<Output = BlueprintResult<PaypalOrder>> + Send;
}
impl PaypalProvider for PaypalClient {
    async fn create_order(
        &self,
        amount_cents: i64,
        currency: &str,
        reference_id: &str,
    ) -> BlueprintResult<PaypalOrder> {
        self.create_order(amount_cents, currency, reference_id)
            .await
    }
    async fn capture_order(&self, id: &str) -> BlueprintResult<PaypalOrder> {
        self.capture_order(id).await
    }
}
pub struct PaypalPaymentService<S, P = PaypalClient> {
    base: BaseEcommerceService<S>,
    client: P,
}
impl<S: EcommerceStore, P: PaypalProvider> PaypalPaymentService<S, P> {
    #[must_use]
    pub const fn new(base: BaseEcommerceService<S>, client: P) -> Self {
        Self { base, client }
    }
    #[must_use]
    pub const fn base(&self) -> &BaseEcommerceService<S> {
        &self.base
    }
}
impl<S: EcommerceStore, P: PaypalProvider> PaymentService for PaypalPaymentService<S, P> {
    type CreatedPayment = PaymentDto;
    async fn create_payment(&self, request: CreatePaymentDto) -> BlueprintResult<PaymentDto> {
        let order = self
            .client
            .create_order(request.amount_cents, &request.currency, &request.order_id)
            .await?;
        self.base
            .create_payment_with_reference(request, Some(order.id))
            .await
    }
    async fn get_payment(&self, id: &str) -> BlueprintResult<PaymentDto> {
        self.base.get_payment(id).await
    }
    async fn confirm_payment(&self, request: ConfirmPaymentDto) -> BlueprintResult<PaymentDto> {
        if !self
            .base
            .claim_for_confirmation(&request.provider_ref)
            .await?
        {
            return self.base.confirm_payment(request).await;
        }
        if let Err(error) = self.client.capture_order(&request.provider_ref).await {
            self.base
                .release_confirmation_claim(&request.provider_ref)
                .await?;
            return Err(error);
        }
        self.base.confirm_payment(request).await
    }
    async fn fail_payment(&self, request: FailPaymentDto) -> BlueprintResult<PaymentDto> {
        self.base.fail_payment(request).await
    }
}
fn config_error() -> BlueprintError {
    BlueprintError::new(
        "paypal_invalid_config",
        "Invalid PayPal credentials or configuration",
    )
}
fn shape_error() -> BlueprintError {
    BlueprintError::new(
        "paypal_invalid_response",
        "PayPal returned an invalid response",
    )
}
