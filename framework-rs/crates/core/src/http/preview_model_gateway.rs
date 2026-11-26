//! Isolated local-preview model capability; no managed usage/compliance claims.
use crate::domain::BlueprintResult;
use crate::http::instance_gateway::InstanceGatewayClient;
use crate::http::instance_gateway::InstanceGatewayRequestError;
use crate::http::provider_http::ProviderResponse;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

/// This is a preview error, not a managed model-gateway API.
pub type PreviewModelGatewayError = InstanceGatewayRequestError;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewModel {
    pub id: String,
}
/// Issued ceilings, never remaining usage, monthly tokens or billing units.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewAllowance {
    pub expires_at: i64,
    pub max_calls: u32,
    pub max_units: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewModels {
    pub models: Vec<PreviewModel>,
    pub preview: PreviewAllowance,
}
/// Reuses instance HMAC, TLS/loopback restrictions and nonretrying transport.
#[derive(Clone)]
pub struct PreviewModelGatewayClient {
    transport: InstanceGatewayClient,
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
}
impl std::fmt::Debug for PreviewModelGatewayClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreviewModelGatewayClient")
            .finish_non_exhaustive()
    }
}
impl PreviewModelGatewayClient {
    #[must_use]
    pub fn new(transport: InstanceGatewayClient) -> Self {
        Self {
            transport,
            clock: Arc::new(|| chrono::Utc::now().timestamp_millis()),
        }
    }
    /// Loads the preview relay's trusted instance configuration.
    /// # Errors
    /// Rejects missing/unsafe transport settings; never falls back to vendor keys.
    pub fn from_env() -> BlueprintResult<Self> {
        Ok(Self::new(InstanceGatewayClient::from_env()?))
    }
    /// Trusted server/test clock for catalog expiration, not HMAC timestamps.
    #[must_use]
    pub fn with_clock(mut self, clock: impl Fn() -> i64 + Send + Sync + 'static) -> Self {
        self.clock = Arc::new(clock);
        self
    }
    /// Fetches and validates one preview model with an exact 8192-byte raw cap.
    /// # Errors
    /// Invalid/expired catalogs report 502. HTTP refusals preserve status/retry
    /// hints, while network failures retain unknown status without remote text.
    pub async fn models(&self) -> Result<PreviewModels, PreviewModelGatewayError> {
        let response = self
            .transport
            .request_json_with_response_limit(Method::GET, &["models"], None, 8192)
            .await
            .map_err(|error| {
                if error.code == "provider_invalid_response" {
                    invalid_catalog()
                } else {
                    error.into()
                }
            })?;
        let body = success(response)?;
        catalog(&body, (self.clock)())
    }
    /// Sends an opaque JSON chat body to the fixed preview route. No streaming
    /// method exists; the relay owns request validation. The response stays JSON
    /// and an omitted `created` timestamp is never manufactured.
    /// # Errors
    /// Preserves sanitized HTTP status/retry hints and transport failures.
    pub async fn chat_completion(
        &self,
        request: &Value,
    ) -> Result<Value, PreviewModelGatewayError> {
        success(
            self.transport
                .request_json_with_response_limit(
                    Method::POST,
                    &["models", "chat", "completions"],
                    Some(request),
                    1024 * 1024,
                )
                .await?,
        )
    }
}
fn success(response: ProviderResponse) -> Result<Value, PreviewModelGatewayError> {
    if response.is_success() {
        Ok(response.body)
    } else {
        Err(PreviewModelGatewayError {
            code: "preview_model_request_failed".into(),
            status: Some(response.status),
            retry_after_seconds: response.retry_after_seconds,
        })
    }
}
fn invalid_catalog() -> PreviewModelGatewayError {
    PreviewModelGatewayError {
        code: "preview_model_catalog_invalid".into(),
        status: Some(502),
        retry_after_seconds: None,
    }
}
fn fields<'a>(
    value: &'a Value,
    allowed: &[&str],
) -> Result<&'a serde_json::Map<String, Value>, PreviewModelGatewayError> {
    let object = value.as_object().ok_or_else(invalid_catalog)?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(invalid_catalog());
    }
    Ok(object)
}
// serde_json's correctly rounded f64 parser is enabled by forklaunch-core/http.
// JSON 1.0 and 1e0 must behave like JS Number.isSafeInteger, not integer tokens.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn integer(value: &Value) -> Result<i64, PreviewModelGatewayError> {
    let number = value.as_f64().ok_or_else(invalid_catalog)?;
    if !number.is_finite() || number.fract() != 0.0 || number.abs() > 9_007_199_254_740_991.0 {
        return Err(invalid_catalog());
    }
    Ok(number as i64)
}
fn catalog(value: &Value, now: i64) -> Result<PreviewModels, PreviewModelGatewayError> {
    fields(value, &["models", "preview"])?;
    fields(&value["preview"], &["expiresAt", "maxCalls", "maxUnits"])?;
    let models = value["models"]
        .as_array()
        .filter(|models| models.len() == 1)
        .ok_or_else(invalid_catalog)?;
    fields(&models[0], &["id"])?;
    let id = models[0]["id"]
        .as_str()
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 160
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_.:-".contains(&byte))
        })
        .ok_or_else(invalid_catalog)?;
    let expires_at = integer(&value["preview"]["expiresAt"])?;
    let max_calls = integer(&value["preview"]["maxCalls"])?;
    let max_units = integer(&value["preview"]["maxUnits"])?;
    if expires_at <= now
        || expires_at > now.checked_add(900_000).ok_or_else(invalid_catalog)?
        || !(1..=20).contains(&max_calls)
        || !(1..=100_000).contains(&max_units)
    {
        return Err(invalid_catalog());
    }
    Ok(PreviewModels {
        models: vec![PreviewModel { id: id.into() }],
        preview: PreviewAllowance {
            expires_at,
            max_calls: u32::try_from(max_calls).map_err(|_| invalid_catalog())?,
            max_units: u32::try_from(max_units).map_err(|_| invalid_catalog())?,
        },
    })
}
#[cfg(test)]
mod tests;
