//! Tools in the MCP `tools/list` shape, with their handlers and call context.
use super::types::ToolAnnotations;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::time::Instant;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Longest tool name; names match `^[a-z][a-z0-9_]{0,63}$`.
pub const MAX_TOOL_NAME: usize = 64;

/// Whether `name` matches `^[a-z][a-z0-9_]{0,63}$`.
#[must_use]
pub fn valid_tool_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    name.len() <= MAX_TOOL_NAME
        && bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// Turns any label into a tool-name segment: `GetRecord` → `get_record`.
#[must_use]
pub fn tool_name_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 4);
    let mut previous: Option<char> = None;
    let mut pending_separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            let boundary = character.is_ascii_uppercase()
                && previous.is_some_and(|p| p.is_ascii_lowercase() || p.is_ascii_digit());
            if (pending_separator || boundary) && !out.is_empty() {
                out.push('_');
            }
            pending_separator = false;
            out.push(character.to_ascii_lowercase());
        } else {
            pending_separator = true;
        }
        previous = Some(character);
    }
    out
}

/// `<prefix>__<segment>`, cut to 64 bytes.
#[must_use]
pub fn prefixed_tool_name(prefix: &str, name: &str) -> String {
    let mut full = format!("{}__{}", tool_name_segment(prefix), tool_name_segment(name));
    full.truncate(MAX_TOOL_NAME);
    full
}

/// A latched cancellation signal shared by one run and its tool calls.
#[derive(Clone, Debug)]
pub struct Cancellation(Arc<watch::Sender<bool>>);
impl Default for Cancellation {
    fn default() -> Self {
        Self(Arc::new(watch::channel(false).0))
    }
}
impl Cancellation {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.send_replace(true);
    }
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        *self.0.borrow()
    }
    /// Resolves once cancelled.
    pub async fn cancelled(&self) {
        let mut receiver = self.0.subscribe();
        // An error means the sender is gone, which cannot happen while `self` lives.
        let _ = receiver.wait_for(|cancelled| *cancelled).await;
    }
}

/// The verified caller of a run: user id, the caller's own `Authorization`
/// header (forwarded to app services) and the run's cancellation.
#[derive(Clone)]
pub struct AgentCaller {
    pub user_id: String,
    pub authorization: Option<String>,
    pub cancellation: Cancellation,
}
impl std::fmt::Debug for AgentCaller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentCaller")
            .field("user_id", &self.user_id)
            .field(
                "authorization",
                &self.authorization.as_ref().map(|_| "[redacted]"),
            )
            .finish_non_exhaustive()
    }
}
impl AgentCaller {
    #[must_use]
    pub fn new(user_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            authorization: None,
            cancellation: Cancellation::new(),
        }
    }
    #[must_use]
    pub fn with_authorization(mut self, authorization: Option<String>) -> Self {
        self.authorization = authorization;
        self
    }
}

/// Per-call context handed to tool handlers.
#[derive(Clone)]
pub struct ToolContext {
    pub user_id: String,
    /// The caller's own header; forward it only to the app's services.
    pub authorization: Option<String>,
    pub cancellation: Cancellation,
    /// When the call times out (the agent's per-tool limit).
    pub deadline: Instant,
    pub span: tracing::Span,
}
impl std::fmt::Debug for ToolContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolContext")
            .field("user_id", &self.user_id)
            .field(
                "authorization",
                &self.authorization.as_ref().map(|_| "[redacted]"),
            )
            .field("deadline", &self.deadline)
            .finish_non_exhaustive()
    }
}
impl ToolContext {
    #[must_use]
    pub fn new(caller: &AgentCaller, deadline: Instant) -> Self {
        Self {
            user_id: caller.user_id.clone(),
            authorization: caller.authorization.clone(),
            cancellation: caller.cancellation.clone(),
            deadline,
            span: tracing::Span::current(),
        }
    }
    /// Time left before the deadline.
    #[must_use]
    pub fn remaining(&self) -> std::time::Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
}

/// A tool failure. `InvalidInput` tells the model its arguments were wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolError {
    pub message: String,
    pub invalid_input: bool,
}
impl ToolError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            invalid_input: false,
        }
    }
    #[must_use]
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            invalid_input: true,
        }
    }
}
impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
/// `?` works on any error; only its message reaches the model.
impl<E: std::error::Error> From<E> for ToolError {
    fn from(error: E) -> Self {
        Self::new(error.to_string())
    }
}

pub type ToolResult = Result<Value, ToolError>;

/// Runs a tool. Implement it for a type, or pass a closure to [`Tool::handler`].
pub trait ToolHandler: Send + Sync + 'static {
    fn call(&self, input: Value, context: ToolContext) -> impl Future<Output = ToolResult> + Send;
}

trait DynHandler: Send + Sync {
    fn call(&self, input: Value, context: ToolContext) -> BoxFuture<'_, ToolResult>;
}
impl<H: ToolHandler> DynHandler for H {
    fn call(&self, input: Value, context: ToolContext) -> BoxFuture<'_, ToolResult> {
        Box::pin(ToolHandler::call(self, input, context))
    }
}

struct FnHandler<F>(F);
impl<F, Fut> ToolHandler for FnHandler<F>
where
    F: Fn(Value, ToolContext) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ToolResult> + Send,
{
    fn call(&self, input: Value, context: ToolContext) -> impl Future<Output = ToolResult> + Send {
        (self.0)(input, context)
    }
}

struct Missing;
impl ToolHandler for Missing {
    async fn call(&self, _: Value, _: ToolContext) -> ToolResult {
        Err(ToolError::new("This tool has no handler."))
    }
}

/// A tool the model can call, in the MCP `tools/list` shape plus its handler.
///
/// ```ignore
/// Tool::new("current_time", "Current time", "The current UTC time.", json!({"type":"object"}))
///     .read_only()
///     .handler(|_, _| async { Ok(json!({"now": "..."})) });
/// ```
#[derive(Clone)]
pub struct Tool {
    pub name: String,
    pub title: Option<String>,
    pub description: String,
    pub input_schema: Value,
    pub annotations: ToolAnnotations,
    handler: Arc<dyn DynHandler>,
}
impl std::fmt::Debug for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tool")
            .field("name", &self.name)
            .field("annotations", &self.annotations)
            .finish_non_exhaustive()
    }
}
impl Tool {
    /// A tool with an explicit JSON Schema input. Without annotations it needs approval.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
    ) -> Self {
        let title = title.into();
        Self {
            name: name.into(),
            title: (!title.is_empty()).then_some(title),
            description: description.into(),
            input_schema,
            annotations: ToolAnnotations::default(),
            handler: Arc::new(Missing),
        }
    }

    /// A tool whose input schema is derived from `I`; pair with [`Self::typed_handler`].
    #[must_use]
    pub fn with_input<I: JsonSchema>(
        name: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self::new(name, title, description, input_schema::<I>())
    }

    /// Declares the tool read-only, so it runs without approval.
    #[must_use]
    pub fn read_only(mut self) -> Self {
        self.annotations.read_only_hint = Some(true);
        self
    }
    /// Marks the tool destructive; the UI warns before approval.
    #[must_use]
    pub fn destructive(mut self) -> Self {
        self.annotations.read_only_hint = Some(false);
        self.annotations.destructive_hint = Some(true);
        self
    }
    #[must_use]
    pub fn idempotent(mut self) -> Self {
        self.annotations.idempotent_hint = Some(true);
        self
    }
    #[must_use]
    pub fn open_world(mut self) -> Self {
        self.annotations.open_world_hint = Some(true);
        self
    }
    #[must_use]
    pub fn annotations(mut self, annotations: ToolAnnotations) -> Self {
        self.annotations = annotations;
        self
    }

    /// Handles raw JSON input.
    #[must_use]
    pub fn handler<F, Fut>(mut self, handler: F) -> Self
    where
        F: Fn(Value, ToolContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ToolResult> + Send + 'static,
    {
        self.handler = Arc::new(FnHandler(handler));
        self
    }

    /// Handles input deserialized into `I`; invalid input is reported to the model.
    #[must_use]
    pub fn typed_handler<I, F, Fut>(self, handler: F) -> Self
    where
        I: DeserializeOwned + Send + 'static,
        F: Fn(I, ToolContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ToolResult> + Send + 'static,
    {
        let handler = Arc::new(handler);
        self.handler(move |input, context| {
            let handler = handler.clone();
            async move {
                let input = serde_json::from_value::<I>(input)
                    .map_err(|error| ToolError::invalid_input(format!("input: {error}")))?;
                handler(input, context).await
            }
        })
    }

    /// Uses a [`ToolHandler`] implementation.
    #[must_use]
    pub fn with_handler(mut self, handler: impl ToolHandler) -> Self {
        self.handler = Arc::new(handler);
        self
    }

    /// Whether the user must approve each call.
    #[must_use]
    pub fn requires_approval(&self) -> bool {
        self.annotations.requires_approval()
    }

    /// Runs the handler.
    /// # Errors
    /// Returns the handler's failure.
    pub async fn call(&self, input: Value, context: ToolContext) -> ToolResult {
        self.handler.call(input, context).await
    }

    /// Checks the name and description of a tool the agent owns.
    /// # Errors
    /// Rejects names outside `^[a-z][a-z0-9_]{0,63}$` and empty descriptions.
    pub fn check(&self) -> Result<(), String> {
        if !valid_tool_name(&self.name) {
            return Err(format!(
                "Agent tool name \"{}\" must be lowercase snake_case, up to 64 characters.",
                self.name
            ));
        }
        if self.description.trim().is_empty() {
            return Err(format!("Agent tool \"{}\" needs a description.", self.name));
        }
        Ok(())
    }
}

/// JSON Schema for a tool input type, always an object schema.
#[must_use]
pub fn input_schema<I: JsonSchema>() -> Value {
    let schema = crate::contract::schema_for::<I>();
    let mut value = schema
        .json_schema
        .as_deref()
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .unwrap_or_else(|| json!({}));
    if let Some(object) = value.as_object_mut() {
        object.remove("$schema");
        object.remove("title");
        object
            .entry("type")
            .or_insert_with(|| Value::String("object".into()));
    }
    value
}
