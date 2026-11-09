//! The model connection behind a trait, so tests can script a model.
use super::tool::BoxFuture;
use crate::http::model_gateway::{ModelGatewayClient, ModelGatewayStream};
use serde_json::Value;
use std::sync::Mutex;

/// A failed model call. Only the HTTP status is kept; gateway text is never shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelError {
    pub status: Option<u16>,
    /// The connection is not configured (missing settings).
    pub unconfigured: bool,
}
impl ModelError {
    #[must_use]
    pub const fn status(status: u16) -> Self {
        Self {
            status: Some(status),
            unconfigured: false,
        }
    }
    #[must_use]
    pub const fn unconfigured() -> Self {
        Self {
            status: None,
            unconfigured: true,
        }
    }
}

/// Incremental chat completion chunks (`choices[].delta`).
pub trait ModelChunks: Send {
    /// The next chunk, or `None` at the end of the stream.
    fn next(&mut self) -> BoxFuture<'_, Result<Option<Value>, ModelError>>;
}

/// OpenAI-style chat completions. Requests and responses are opaque JSON.
pub trait ModelBackend: Send + Sync + 'static {
    /// `POST /models/chat/completions`.
    fn complete(&self, request: Value) -> BoxFuture<'_, Result<Value, ModelError>>;
    /// `POST /models/chat/completions/stream`.
    fn stream(&self, request: Value) -> BoxFuture<'_, Result<Box<dyn ModelChunks>, ModelError>>;
}

/// The managed model gateway, configured from `PLATFORM_GATEWAY_URL`,
/// `INSTANCE_ID` and `INSTANCE_HMAC_KEY` on first use. Local previews point the
/// same settings at the preview relay and set `FORKLAUNCH_MODEL_STREAMING=0`.
#[derive(Default)]
pub struct GatewayModel {
    client: Mutex<Option<ModelGatewayClient>>,
}
impl std::fmt::Debug for GatewayModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GatewayModel").finish_non_exhaustive()
    }
}
impl GatewayModel {
    #[must_use]
    pub fn new(client: ModelGatewayClient) -> Self {
        Self {
            client: Mutex::new(Some(client)),
        }
    }
    fn client(&self) -> Result<ModelGatewayClient, ModelError> {
        let mut slot = self
            .client
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(client) = slot.as_ref() {
            return Ok(client.clone());
        }
        let client = ModelGatewayClient::from_env().map_err(|_| ModelError::unconfigured())?;
        *slot = Some(client.clone());
        Ok(client)
    }
}
impl ModelBackend for GatewayModel {
    fn complete(&self, request: Value) -> BoxFuture<'_, Result<Value, ModelError>> {
        Box::pin(async move {
            let client = self.client()?;
            client
                .chat_completion(&request)
                .await
                .map_err(|error| ModelError {
                    status: error.status,
                    unconfigured: false,
                })
        })
    }
    fn stream(&self, request: Value) -> BoxFuture<'_, Result<Box<dyn ModelChunks>, ModelError>> {
        Box::pin(async move {
            let client = self.client()?;
            let stream = client
                .chat_completion_stream(&request)
                .await
                .map_err(|error| ModelError {
                    status: error.status,
                    unconfigured: false,
                })?;
            Ok(Box::new(GatewayChunks(stream)) as Box<dyn ModelChunks>)
        })
    }
}

struct GatewayChunks(ModelGatewayStream);
impl ModelChunks for GatewayChunks {
    fn next(&mut self) -> BoxFuture<'_, Result<Option<Value>, ModelError>> {
        Box::pin(async move {
            self.0.next().await.map_err(|error| ModelError {
                status: error.status,
                unconfigured: false,
            })
        })
    }
}
