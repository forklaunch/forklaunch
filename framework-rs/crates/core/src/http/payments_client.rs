//! Managed Stripe Connect onboarding and readiness without vendor credentials.
use crate::domain::BlueprintError;
use crate::http::instance_gateway::InstanceGatewayClient;
use reqwest::Method;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentsStatus {
    pub charges_enabled: bool,
    pub payouts_enabled: bool,
    pub requirements_due: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaymentsOnboardingLink {
    pub url: String,
}
/// Sanitized failure: remote response bodies and trusted credentials are omitted.
#[derive(Debug, Clone, PartialEq)]
pub struct PaymentsRequestError {
    pub code: String,
    pub status: Option<u16>,
    pub retry_after_seconds: Option<f64>,
}
impl std::fmt::Display for PaymentsRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Payments request failed ({})", self.code)?;
        if let Some(status) = self.status {
            write!(f, " (status {status})")?;
        }
        Ok(())
    }
}
impl std::error::Error for PaymentsRequestError {}
impl From<BlueprintError> for PaymentsRequestError {
    fn from(error: BlueprintError) -> Self {
        Self {
            code: error.code,
            status: None,
            retry_after_seconds: None,
        }
    }
}
fn invalid(code: &str) -> PaymentsRequestError {
    PaymentsRequestError {
        code: code.into(),
        status: None,
        retry_after_seconds: None,
    }
}
#[derive(Debug, Clone)]
pub struct PaymentsClient {
    gateway: InstanceGatewayClient,
}
impl PaymentsClient {
    #[must_use]
    pub const fn new(gateway: InstanceGatewayClient) -> Self {
        Self { gateway }
    }
    /// # Errors
    /// Rejects invalid redirect URLs and gateway/response failures. Each call is
    /// signed with a fresh nonce; creation is never automatically retried.
    pub async fn onboarding_link(
        &self,
        return_url: &str,
        refresh_url: &str,
    ) -> Result<PaymentsOnboardingLink, PaymentsRequestError> {
        if !absolute_http_url(return_url) || !absolute_http_url(refresh_url) {
            return Err(invalid("payments_invalid_redirect_url"));
        }
        let link: PaymentsOnboardingLink = self
            .call(
                Method::POST,
                "onboarding-link",
                Some(&json!({"returnUrl":return_url,"refreshUrl":refresh_url})),
            )
            .await?;
        if !absolute_http_url(&link.url) {
            return Err(invalid("payments_invalid_response"));
        }
        Ok(link)
    }
    /// # Errors
    /// Returns a sanitized gateway failure or rejects a malformed readiness DTO.
    pub async fn status(&self) -> Result<PaymentsStatus, PaymentsRequestError> {
        self.call(Method::GET, "status", None).await
    }
    async fn call<T: DeserializeOwned>(
        &self,
        method: Method,
        endpoint: &str,
        body: Option<&Value>,
    ) -> Result<T, PaymentsRequestError> {
        let response = self
            .gateway
            .request_json(method, &["payments", endpoint], body)
            .await?;
        if !response.is_success() {
            return Err(PaymentsRequestError {
                code: "payments_request_failed".into(),
                status: Some(response.status),
                retry_after_seconds: response.retry_after_seconds,
            });
        }
        serde_json::from_value(response.body).map_err(|_| invalid("payments_invalid_response"))
    }
}
fn absolute_http_url(value: &str) -> bool {
    if value.len() > 8192 || value.chars().any(char::is_control) {
        return false;
    }
    url::Url::parse(value).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    })
}
