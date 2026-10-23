//! Typed additional MCP tools use the same serde/garde validation as HTTP schemas.
use axum::http::HeaderMap;
use forklaunch_validator::FrameworkSchema;
use rmcp::{
    ErrorData,
    model::{CallToolResponse, CallToolResult, ContentBlock, Tool},
};
use serde_json::{Map, Value};
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

type ManualFuture = Pin<Box<dyn Future<Output = Result<Value, ManualCallError>> + Send>>;
pub(super) enum ManualCallError {
    Arguments,
    Execution(String),
}

pub(super) type ManualHandler =
    Arc<dyn Fn(Map<String, Value>, HeaderMap) -> ManualFuture + Send + Sync>;

/// An application error with an explicitly public message. Keep provider errors,
/// credentials and private payloads out of this message.
pub struct ManualToolError {
    message: String,
}
impl ManualToolError {
    #[must_use]
    pub fn new(public_message: impl Into<String>) -> Self {
        Self {
            message: public_message.into(),
        }
    }
}

/// A manually bound runtime tool. Input schema and runtime validation come from
/// one Rust type; credentials are supplied fresh for each invocation.
pub struct ManualTool {
    pub(super) definition: Tool,
    pub(super) handler: ManualHandler,
}
impl ManualTool {
    /// Binds a typed application handler. The handler must authorize its own
    /// operation using request headers or trusted application state. HTTP route
    /// guards do not automatically apply to additional tools.
    /// # Errors
    /// Rejects empty names and input types without an object JSON schema.
    pub fn typed<I, O, H, F>(name: &str, summary: &str, handler: H) -> Result<Self, String>
    where
        I: FrameworkSchema + Send + 'static,
        O: serde::Serialize,
        H: Fn(I, HeaderMap) -> F + Send + Sync + 'static,
        F: Future<Output = Result<O, ManualToolError>> + Send + 'static,
    {
        if name.is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
            return Err("Invalid MCP tool name".into());
        }
        let definition =
            crate::mcp::McpToolDefinition::manual(name, summary, I::json_schema_json());
        let definition: Tool = serde_json::from_str(&definition.to_json())
            .map_err(|_| "Invalid manual MCP tool schema")?;
        if definition.input_schema.get("type").and_then(Value::as_str) != Some("object") {
            return Err("Manual MCP tool input must be an object".into());
        }
        let handler = Arc::new(handler);
        Ok(Self {
            definition,
            handler: Arc::new(move |arguments, headers| {
                let handler = handler.clone();
                Box::pin(async move {
                    let input: I = serde_json::from_value(Value::Object(arguments))
                        .map_err(|_| ManualCallError::Arguments)?;
                    forklaunch_validator::validate_value(&input)
                        .map_err(|_| ManualCallError::Arguments)?;
                    let output = handler(input, headers)
                        .await
                        .map_err(|error| ManualCallError::Execution(error.message))?;
                    serde_json::to_value(output).map_err(|_| {
                        ManualCallError::Execution("Tool response serialization failed".into())
                    })
                })
            }),
        })
    }
}

pub(super) async fn execute(
    handler: &ManualHandler,
    args: Map<String, Value>,
    headers: HeaderMap,
    timeout: Duration,
    max_bytes: usize,
) -> Result<CallToolResponse, ErrorData> {
    let output = match tokio::time::timeout(timeout, handler(args, headers)).await {
        Ok(Ok(value)) => value,
        Ok(Err(ManualCallError::Arguments)) => {
            return Err(ErrorData::invalid_params("Invalid tool arguments", None));
        }
        Ok(Err(ManualCallError::Execution(message))) => {
            let message = if message.len() > max_bytes {
                "Manual tool error exceeds MCP limit".to_owned()
            } else {
                message
            };
            return Ok(CallToolResult::error(vec![ContentBlock::text(message)]).into());
        }
        Err(_) => {
            return Ok(
                CallToolResult::error(vec![ContentBlock::text("Manual tool timed out")]).into(),
            );
        }
    };
    let text = serde_json::to_string(&output)
        .map_err(|_| ErrorData::internal_error("Tool response serialization failed", None))?;
    if text.len() > max_bytes {
        return Ok(CallToolResult::error(vec![ContentBlock::text(
            "Manual tool response exceeds MCP limit",
        )])
        .into());
    }
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    if output.is_object() {
        result.structured_content = Some(output);
    }
    Ok(result.into())
}

#[cfg(test)]
mod tests;
