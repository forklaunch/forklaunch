//! Managed outbound voice calls using catalog flow names and instance signing.
use crate::http::instance_gateway::InstanceGatewayClient;
use crate::http::instance_gateway::InstanceGatewayRequestError;
use crate::http::sms_client::is_e164;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
pub type VoiceRequestError = InstanceGatewayRequestError;
pub const VOICE_MAX_ATTRIBUTE_KEYS: usize = 20;
pub const VOICE_MAX_ATTRIBUTE_KEY_LENGTH: usize = 64;
pub const VOICE_MAX_ATTRIBUTE_VALUE_LENGTH: usize = 256;
pub const VOICE_MAX_ATTRIBUTE_BYTES: usize = 4096;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StartOutboundCallRequest {
    pub to: String,
    pub flow: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<BTreeMap<String, String>>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StartOutboundCallResult {
    pub call_id: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VoiceCallStatus {
    Initiated,
    InProgress,
    Ended,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VoiceCall {
    pub status: VoiceCallStatus,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disconnect_reason: Option<String>,
}
/// Returns the reason code for refusal, before any gateway call.
#[must_use]
pub fn voice_request_problem(request: &StartOutboundCallRequest) -> Option<&'static str> {
    if !is_e164(&request.to) {
        return Some("voice_invalid_recipient");
    }
    let flow = request.flow.as_bytes();
    if flow.is_empty()
        || flow.len() > 64
        || !flow[0].is_ascii_lowercase()
        || !flow
            .iter()
            .all(|v| v.is_ascii_lowercase() || v.is_ascii_digit() || matches!(v, b'_' | b'-'))
    {
        return Some("voice_invalid_flow");
    }
    if let Some(attributes) = &request.attributes {
        if attributes.len() > VOICE_MAX_ATTRIBUTE_KEYS {
            return Some("voice_too_many_attributes");
        }
        let mut total = 0;
        for (key, value) in attributes {
            let bytes = key.as_bytes();
            if bytes.is_empty()
                || bytes.len() > VOICE_MAX_ATTRIBUTE_KEY_LENGTH
                || !bytes[0].is_ascii_alphabetic()
                || !bytes
                    .iter()
                    .all(|v| v.is_ascii_alphanumeric() || *v == b'_')
            {
                return Some("voice_invalid_attribute_key");
            }
            if key.to_ascii_lowercase().starts_with("fl_") {
                return Some("voice_reserved_attribute_key");
            }
            if value.encode_utf16().count() > VOICE_MAX_ATTRIBUTE_VALUE_LENGTH {
                return Some("voice_attribute_value_too_long");
            }
            total += key.len() + value.len();
        }
        if total > VOICE_MAX_ATTRIBUTE_BYTES {
            return Some("voice_attributes_too_large");
        }
    }
    None
}
#[derive(Debug, Clone)]
pub struct VoiceClient {
    gateway: InstanceGatewayClient,
}
impl VoiceClient {
    #[must_use]
    pub const fn new(gateway: InstanceGatewayClient) -> Self {
        Self { gateway }
    }
    /// # Errors
    /// Rejects invalid flow/recipient/attributes and gateway/response failures.
    pub async fn start_outbound_call(
        &self,
        request: &StartOutboundCallRequest,
    ) -> Result<StartOutboundCallResult, VoiceRequestError> {
        if let Some(problem) = voice_request_problem(request) {
            return Err(invalid(problem));
        }
        let mut body = json!({"to":request.to,"flow":request.flow});
        if let Some(attributes) = &request.attributes {
            body["attributes"] = json!(attributes);
        }
        self.gateway
            .json(Method::POST, &["voice", "calls"], Some(&body))
            .await
    }
    /// # Errors
    /// Rejects invalid call IDs and gateway/typed-response failures.
    pub async fn call(&self, call_id: &str) -> Result<VoiceCall, VoiceRequestError> {
        validate_call_id(call_id)?;
        self.gateway
            .json(Method::GET, &["voice", "calls", call_id], None)
            .await
    }
    /// # Errors
    /// Rejects invalid call IDs and gateway failures. Empty successful responses
    /// are accepted, matching the TypeScript voice transport.
    pub async fn end_call(&self, call_id: &str) -> Result<(), VoiceRequestError> {
        validate_call_id(call_id)?;
        let response = self
            .gateway
            .request_json_or_empty(Method::DELETE, &["voice", "calls", call_id])
            .await?;
        if !response.is_success() {
            return Err(VoiceRequestError {
                code: "instance_gateway_request_failed".into(),
                status: Some(response.status),
                retry_after_seconds: response.retry_after_seconds,
            });
        }
        Ok(())
    }
}
fn validate_call_id(call_id: &str) -> Result<(), VoiceRequestError> {
    if call_id.is_empty()
        || call_id.len() > 128
        || !call_id
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || matches!(v, b'_' | b'-'))
    {
        return Err(invalid("voice_invalid_call_id"));
    }
    Ok(())
}
fn invalid(code: &str) -> VoiceRequestError {
    let mut error = VoiceRequestError::validation(code);
    error.status = Some(400);
    error
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VoiceCallStartedEvent {
    pub call_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VoiceCallEndedEvent {
    pub call_id: String,
    pub duration_seconds: f64,
    pub disconnect_reason: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VoiceRecordingReadyEvent {
    pub call_id: String,
    pub recording_key: String,
}
