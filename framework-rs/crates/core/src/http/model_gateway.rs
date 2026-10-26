//! Managed model catalog and chat over the instance-signed gateway.
mod dto;
mod stream;
use crate::domain::BlueprintResult;
use crate::http::instance_gateway::InstanceGatewayClient;
use crate::http::instance_gateway::InstanceGatewayRequestError;
pub use dto::{
    GatewayChatChunk, GatewayChatChunkChoice, GatewayChatCompletion, GatewayChatCompletionChoice,
    GatewayChatDelta, GatewayChatMessage, GatewayChatRequest, GatewayChatRole,
    GatewayCompletionMessage, GatewayModel, GatewayModels, GatewayPresence, GatewayUsage,
};
use reqwest::Method;
use serde_json::Value;
pub use stream::{ModelGatewayStream, ModelStreamLimits};

/// Sanitized gateway refusal or transport failure, with status/retry metadata.
pub type ModelGatewayError = InstanceGatewayRequestError;

/// Uses the configured platform identity and fixed model capability routes.
/// Catalog and completion JSON remain opaque, matching the TypeScript client's
/// unchecked response casts. Budget and compliance fields are platform claims;
/// this client does not independently enforce or verify them.
#[derive(Clone, Debug)]
pub struct ModelGatewayClient {
    transport: InstanceGatewayClient,
}

impl ModelGatewayClient {
    #[must_use]
    pub const fn new(transport: InstanceGatewayClient) -> Self {
        Self { transport }
    }

    /// Loads managed instance configuration; never falls back to provider keys.
    /// # Errors
    /// Rejects missing or unsafe transport settings.
    pub fn from_env() -> BlueprintResult<Self> {
        Ok(Self::new(InstanceGatewayClient::from_env()?))
    }

    /// Reads the platform's managed model catalog without projecting fields.
    /// # Errors
    /// Returns sanitized refusals, invalid JSON, or transport failures. Success
    /// responses have a one-MiB raw bound, including chunked responses.
    pub async fn models(&self) -> Result<Value, ModelGatewayError> {
        self.send(Method::GET, &["models"], None).await
    }

    /// Sends an opaque JSON request to the fixed nonstreaming completion route.
    /// Unknown request/response fields and omitted timestamps remain intact.
    /// The platform owns model and request validation.
    /// # Errors
    /// Returns sanitized refusals, invalid JSON, or transport failures without
    /// retrying a request that may have consumed model allowance.
    pub async fn chat_completion(&self, request: &Value) -> Result<Value, ModelGatewayError> {
        self.send(
            Method::POST,
            &["models", "chat", "completions"],
            Some(request),
        )
        .await
    }

    /// Opens the fixed signed chat stream using bounded default limits.
    /// Drop or explicitly cancel the stream to close its native response body.
    /// # Errors
    /// Returns sanitized status/retry metadata or transport errors without retries.
    pub async fn chat_completion_stream(
        &self,
        request: &Value,
    ) -> Result<ModelGatewayStream, ModelGatewayError> {
        self.chat_completion_stream_with_limits(request, ModelStreamLimits::default())
            .await
    }
    /// Opens a chat stream with explicit validated byte/frame/time limits.
    /// # Errors
    /// Returns sanitized gateway refusals or transport errors without retries.
    pub async fn chat_completion_stream_with_limits(
        &self,
        request: &Value,
        limits: ModelStreamLimits,
    ) -> Result<ModelGatewayStream, ModelGatewayError> {
        let response = self
            .transport
            .request_stream(
                &["models", "chat", "completions", "stream"],
                request,
                limits.total_bytes(),
                limits.timeout(),
            )
            .await?;
        ModelGatewayStream::new(response, limits)
    }

    async fn send(
        &self,
        method: Method,
        route: &[&str],
        body: Option<&Value>,
    ) -> Result<Value, ModelGatewayError> {
        let response = self
            .transport
            .request_json_with_response_limit(method, route, body, 1024 * 1024)
            .await?;
        if response.is_success() {
            Ok(response.body)
        } else {
            Err(ModelGatewayError {
                code: "model_gateway_request_failed".into(),
                status: Some(response.status),
                retry_after_seconds: response.retry_after_seconds,
            })
        }
    }
}
