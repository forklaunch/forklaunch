//! Minimal MCP client over streamable HTTP: `initialize`, `tools/list` and
//! `tools/call`. Responses may be `application/json` or `text/event-stream`.
//! stdio servers are not supported. Header values are never formatted.
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method, Response, StatusCode, redirect::Policy};
use serde_json::{Value, json};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use url::Url;

/// Preferred protocol revision; older servers answer with their own.
pub const PROTOCOL_VERSION: &str = "2025-06-18";
const FALLBACK_PROTOCOL_VERSIONS: [&str; 2] = ["2025-03-26", "2024-11-05"];
const SESSION_HEADER: &str = "mcp-session-id";
const VERSION_HEADER: &str = "mcp-protocol-version";
/// Tools read across all `tools/list` pages.
pub const MAX_LISTED_TOOLS: usize = 500;

/// A sanitized client failure. Messages never carry header values or bodies
/// beyond a short JSON-RPC error message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpClientError {
    pub message: String,
}
impl McpClientError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
impl std::fmt::Display for McpClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for McpClientError {}

/// One `tools/list` entry, as the server sent it.
#[derive(Clone, Debug, PartialEq)]
pub struct McpTool {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub input_schema: Value,
    /// Raw MCP annotations object, when present.
    pub annotations: Option<Value>,
}

/// A `tools/call` result.
#[derive(Clone, Debug, PartialEq)]
pub struct McpCallResult {
    pub content: Value,
    pub structured_content: Option<Value>,
    pub is_error: bool,
}

/// Connection settings for one streamable HTTP endpoint.
#[derive(Clone)]
pub struct McpClientOptions {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub timeout: Duration,
    pub max_response_bytes: usize,
    pub client_name: String,
    pub client_version: String,
}
impl std::fmt::Debug for McpClientOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClientOptions")
            .field("url", &self.url)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}
impl McpClientOptions {
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            headers: Vec::new(),
            timeout: Duration::from_secs(15),
            max_response_bytes: 4 * 1024 * 1024,
            client_name: "forklaunch-agent".into(),
            client_version: env!("CARGO_PKG_VERSION").into(),
        }
    }
    #[must_use]
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
    #[must_use]
    pub const fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

/// An initialized session. Drop it or call [`McpClient::close`] when done.
pub struct McpClient {
    http: Client,
    url: Url,
    headers: HeaderMap,
    max_response_bytes: usize,
    session: Mutex<Option<HeaderValue>>,
    protocol_version: Mutex<String>,
    next_id: AtomicU64,
}
impl std::fmt::Debug for McpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClient")
            .field("url", &self.url.as_str())
            .finish_non_exhaustive()
    }
}

impl McpClient {
    /// Connects and runs the `initialize` handshake, falling back to older
    /// protocol revisions when the server rejects the preferred one.
    /// # Errors
    /// Rejects invalid URLs/headers, transport failures and handshake errors.
    pub async fn connect(options: McpClientOptions) -> Result<Self, McpClientError> {
        let url = Url::parse(&options.url)
            .ok()
            .filter(|url| matches!(url.scheme(), "http" | "https"))
            .ok_or_else(|| McpClientError::new("The MCP server URL is not a valid http(s) URL."))?;
        let mut headers = HeaderMap::new();
        for (name, value) in &options.headers {
            let name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| McpClientError::new("An MCP server header name is invalid."))?;
            let mut value = HeaderValue::from_str(value)
                .map_err(|_| McpClientError::new("An MCP server header value is invalid."))?;
            value.set_sensitive(true);
            headers.insert(name, value);
        }
        let http = Client::builder()
            .redirect(Policy::none())
            .timeout(options.timeout)
            .build()
            .map_err(|_| McpClientError::new("The MCP client could not be created."))?;
        let client = Self {
            http,
            url,
            headers,
            max_response_bytes: options.max_response_bytes,
            session: Mutex::new(None),
            protocol_version: Mutex::new(PROTOCOL_VERSION.into()),
            next_id: AtomicU64::new(1),
        };
        let info = json!({"name": options.client_name, "version": options.client_version});
        let mut last = None;
        for version in std::iter::once(PROTOCOL_VERSION).chain(FALLBACK_PROTOCOL_VERSIONS) {
            let params =
                json!({"protocolVersion": version, "capabilities": {}, "clientInfo": info});
            match client.request("initialize", params, true).await {
                Ok(result) => {
                    let agreed = result
                        .get("protocolVersion")
                        .and_then(Value::as_str)
                        .unwrap_or(version)
                        .to_owned();
                    *lock(&client.protocol_version) = agreed;
                    client.notify("notifications/initialized").await?;
                    return Ok(client);
                }
                Err(Failure::Rpc(error)) => last = Some(error),
                Err(Failure::Transport(error)) => return Err(error),
            }
        }
        Err(last.unwrap_or_else(|| McpClientError::new("The MCP server refused to initialize.")))
    }

    /// Agreed protocol revision.
    #[must_use]
    pub fn protocol_version(&self) -> String {
        lock(&self.protocol_version).clone()
    }

    /// Lists every tool, following `nextCursor` up to [`MAX_LISTED_TOOLS`].
    /// # Errors
    /// Returns transport, JSON-RPC or malformed-result failures.
    pub async fn list_tools(&self) -> Result<Vec<McpTool>, McpClientError> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let params = cursor
                .as_ref()
                .map_or_else(|| json!({}), |cursor| json!({"cursor": cursor}));
            let result = self.call("tools/list", params).await?;
            let page = result
                .get("tools")
                .and_then(Value::as_array)
                .ok_or_else(|| McpClientError::new("The MCP server sent an invalid tool list."))?;
            for tool in page {
                tools.push(parse_tool(tool)?);
            }
            cursor = result
                .get("nextCursor")
                .and_then(Value::as_str)
                .filter(|next| !next.is_empty())
                .map(str::to_owned);
            if cursor.is_none() || tools.len() >= MAX_LISTED_TOOLS {
                tools.truncate(MAX_LISTED_TOOLS);
                return Ok(tools);
            }
        }
    }

    /// Calls one tool with JSON object arguments.
    /// # Errors
    /// Returns transport, JSON-RPC or malformed-result failures. A tool that
    /// reports `isError` is a successful call with `is_error: true`.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: Value,
    ) -> Result<McpCallResult, McpClientError> {
        let result = self
            .call("tools/call", json!({"name": name, "arguments": arguments}))
            .await?;
        Ok(McpCallResult {
            content: result
                .get("content")
                .cloned()
                .unwrap_or(Value::Array(Vec::new())),
            structured_content: result
                .get("structuredContent")
                .filter(|value| !value.is_null())
                .cloned(),
            is_error: result.get("isError").and_then(Value::as_bool) == Some(true),
        })
    }

    /// Ends the server session when one was issued. Failures are ignored.
    pub async fn close(&self) {
        let Some(session) = lock(&self.session).clone() else {
            return;
        };
        let _ = self
            .http
            .request(Method::DELETE, self.url.clone())
            .headers(self.headers.clone())
            .header(SESSION_HEADER, session)
            .send()
            .await;
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, McpClientError> {
        self.request(method, params, false)
            .await
            .map_err(Failure::into_error)
    }

    async fn notify(&self, method: &str) -> Result<(), McpClientError> {
        let response = self
            .send(&json!({"jsonrpc": "2.0", "method": method}), false)
            .await?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(status_error(response.status()))
        }
    }

    async fn request(
        &self,
        method: &str,
        params: Value,
        initialize: bool,
    ) -> Result<Value, Failure> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let body = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let response = self
            .send(&body, initialize)
            .await
            .map_err(Failure::Transport)?;
        let status = response.status();
        if !status.is_success() {
            return Err(Failure::Transport(status_error(status)));
        }
        if initialize && let Some(session) = response.headers().get(SESSION_HEADER) {
            *lock(&self.session) = Some(session.clone());
        }
        let event_stream = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("text/event-stream"));
        let bytes = self.read(response).await.map_err(Failure::Transport)?;
        let message = if event_stream {
            sse_message(&bytes, id)
        } else {
            serde_json::from_slice::<Value>(&bytes).ok()
        }
        .ok_or_else(|| {
            Failure::Transport(McpClientError::new(
                "The MCP server sent an invalid response.",
            ))
        })?;
        if let Some(error) = message.get("error") {
            let text = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("request failed");
            let text: String = text.chars().take(200).collect();
            return Err(Failure::Rpc(McpClientError::new(format!(
                "The MCP server answered with an error: {text}"
            ))));
        }
        message.get("result").cloned().ok_or_else(|| {
            Failure::Transport(McpClientError::new("The MCP server sent no result."))
        })
    }

    async fn send(&self, body: &Value, initialize: bool) -> Result<Response, McpClientError> {
        let mut request = self
            .http
            .post(self.url.clone())
            .headers(self.headers.clone())
            .header(ACCEPT, "application/json, text/event-stream")
            .json(body);
        if !initialize {
            request = request.header(VERSION_HEADER, self.protocol_version());
            if let Some(session) = lock(&self.session).clone() {
                request = request.header(SESSION_HEADER, session);
            }
        }
        request.send().await.map_err(|error| {
            McpClientError::new(if error.is_timeout() {
                "The MCP server did not answer in time."
            } else {
                "The MCP server could not be reached."
            })
        })
    }

    async fn read(&self, mut response: Response) -> Result<Vec<u8>, McpClientError> {
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| McpClientError::new("The MCP server response was interrupted."))?
        {
            if bytes.len() + chunk.len() > self.max_response_bytes {
                return Err(McpClientError::new("The MCP server response is too large."));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
}

enum Failure {
    Rpc(McpClientError),
    Transport(McpClientError),
}
impl Failure {
    fn into_error(self) -> McpClientError {
        match self {
            Self::Rpc(error) | Self::Transport(error) => error,
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn status_error(status: StatusCode) -> McpClientError {
    McpClientError::new(match status.as_u16() {
        401 | 403 => "The MCP server refused the credentials.".to_owned(),
        404 => "The MCP server endpoint or session was not found.".to_owned(),
        code => format!("The MCP server answered {code}."),
    })
}

fn parse_tool(value: &Value) -> Result<McpTool, McpClientError> {
    let invalid = || McpClientError::new("The MCP server sent an invalid tool.");
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
    Ok(McpTool {
        name: name.to_owned(),
        title: text("title"),
        description: text("description"),
        input_schema: value
            .get("inputSchema")
            .filter(|schema| schema.is_object())
            .cloned()
            .unwrap_or_else(|| json!({"type": "object"})),
        annotations: value.get("annotations").filter(|a| a.is_object()).cloned(),
    })
}

/// Finds the JSON-RPC response with `id` among SSE `data:` events.
fn sse_message(bytes: &[u8], id: u64) -> Option<Value> {
    let text = String::from_utf8_lossy(bytes);
    let mut data = String::new();
    let mut found = None;
    let mut flush = |data: &mut String| {
        if !data.is_empty() {
            if let Ok(message) = serde_json::from_str::<Value>(data)
                && message.get("id").and_then(Value::as_u64) == Some(id)
            {
                found = Some(message);
            }
            data.clear();
        }
    };
    for line in text.lines() {
        if line.is_empty() {
            flush(&mut data);
        } else if let Some(value) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(value.strip_prefix(' ').unwrap_or(value));
        }
    }
    flush(&mut data);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_messages_are_matched_by_id() {
        let body = b"event: message\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\n\nid: 1\ndata: {\"jsonrpc\":\"2.0\",\"id\":7,\n data:\"result\":{}}\n\ndata: {\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{\"ok\":true}}\n\n";
        assert_eq!(sse_message(body, 3).unwrap()["result"]["ok"], true);
        assert!(sse_message(body, 9).is_none());
    }

    #[test]
    fn tools_default_to_an_object_schema() {
        let tool = parse_tool(&json!({"name": "search"})).unwrap();
        assert_eq!(tool.input_schema, json!({"type": "object"}));
        assert!(parse_tool(&json!({"title": "x"})).is_err());
    }
}
