//! Tool sources: the agent's own code, the app's services (from their `OpenAPI`
//! documents, over the main HTTP port) and external MCP servers.
use super::tool::{BoxFuture, Tool, ToolContext, ToolError, prefixed_tool_name};
use super::types::{ToolAnnotations, ToolSourceKind};
use crate::mcp::client::{McpClient, McpClientOptions};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::{Client, Method, redirect::Policy};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;

/// How long a remote source's tool list is reused.
pub const SOURCE_REFRESH: Duration = Duration::from_mins(5);
const OPENAPI_TIMEOUT: Duration = Duration::from_secs(10);
const MCP_LIST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;
const MAX_SERVICE_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const METHODS: [&str; 5] = ["get", "post", "put", "patch", "delete"];
// Path segments stay readable; everything else outside unreserved is escaped.
const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

/// A provider of tools.
pub trait ToolSource: Send + Sync + 'static {
    fn kind(&self) -> ToolSourceKind;
    fn name(&self) -> &str;
    /// Tool definitions. Remote sources reuse their list for [`SOURCE_REFRESH`]
    /// unless `refresh` is set.
    fn list<'a>(
        &'a self,
        context: &'a ToolContext,
        refresh: bool,
    ) -> BoxFuture<'a, Result<Vec<Tool>, String>>;
}

/// The agent's own tools.
#[derive(Clone, Debug)]
pub struct LocalTools {
    name: String,
    tools: Vec<Tool>,
}
impl LocalTools {
    /// # Errors
    /// Rejects invalid names, empty descriptions and duplicate names.
    pub fn new(name: impl Into<String>, tools: Vec<Tool>) -> Result<Self, String> {
        let mut seen = BTreeSet::new();
        for tool in &tools {
            tool.check()?;
            if !seen.insert(tool.name.clone()) {
                return Err(format!("Two agent tools are named \"{}\".", tool.name));
            }
        }
        Ok(Self {
            name: name.into(),
            tools,
        })
    }
}
impl ToolSource for LocalTools {
    fn kind(&self) -> ToolSourceKind {
        ToolSourceKind::Local
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn list<'a>(&'a self, _: &'a ToolContext, _: bool) -> BoxFuture<'a, Result<Vec<Tool>, String>> {
        Box::pin(async move { Ok(self.tools.clone()) })
    }
}

struct Cache<T> {
    value: Mutex<Option<(Instant, T)>>,
}
impl<T: Clone> Cache<T> {
    const fn new() -> Self {
        Self {
            value: Mutex::new(None),
        }
    }
    fn get(&self) -> Option<T> {
        let slot = self
            .value
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        slot.as_ref()
            .filter(|(at, _)| at.elapsed() < SOURCE_REFRESH)
            .map(|(_, value)| value.clone())
    }
    fn set(&self, value: T) {
        *self
            .value
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((Instant::now(), value));
    }
}

/// Reads a setting, treating an empty value as unset.
#[must_use]
pub fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// `Bearer <value>` from a setting, for MCP server headers.
#[must_use]
pub fn env_bearer(name: &str) -> Option<String> {
    env(name).map(|token| format!("Bearer {token}"))
}

/// `records` → `RECORDS_URL`; `case-notes` → `CASE_NOTES_URL`.
#[must_use]
pub fn service_url_setting(service: &str) -> String {
    format!("{}_URL", service.replace('-', "_").to_ascii_uppercase())
}

/// Exposes an app service's routes as tools, read from its `OpenAPI` document
/// and called over its normal HTTP port with the caller's `Authorization`
/// header, so the service's own auth and roles apply.
///
/// Never exposed, whatever [`ServiceTools::include`] says: `access: internal`
/// and HMAC-signed routes (a forwarded user token can never call them), and
/// routes whose request or success response is not JSON (SSE streams,
/// downloads, multipart uploads). Use [`ServiceTools::include`] as an
/// allowlist and [`ServiceTools::read_only`] to drop writes.
#[must_use]
pub fn service_tools(service: impl Into<String>) -> ServiceTools {
    ServiceTools {
        service: service.into(),
        url: None,
        version: None,
        include: None,
        exclude: BTreeSet::new(),
        read_only: false,
        http: http_client(),
        cache: Arc::new(Cache::new()),
    }
}

fn http_client() -> Client {
    Client::builder()
        .redirect(Policy::none())
        .build()
        .unwrap_or_default()
}

/// See [`service_tools`].
#[derive(Clone)]
pub struct ServiceTools {
    service: String,
    url: Option<String>,
    version: Option<String>,
    include: Option<BTreeSet<String>>,
    exclude: BTreeSet<String>,
    read_only: bool,
    http: Client,
    cache: Arc<Cache<Vec<Tool>>>,
}
impl std::fmt::Debug for ServiceTools {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceTools")
            .field("service", &self.service)
            .finish_non_exhaustive()
    }
}
impl ServiceTools {
    /// Base URL instead of the `<SERVICE>_URL` setting.
    #[must_use]
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }
    /// Version segment of `/api/<version>/openapi`. Default: `VERSION` or `v1`.
    #[must_use]
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }
    /// Only these operation ids.
    #[must_use]
    pub fn include<I: IntoIterator<Item = S>, S: Into<String>>(mut self, ids: I) -> Self {
        self.include = Some(ids.into_iter().map(Into::into).collect());
        self
    }
    /// Never these operation ids.
    #[must_use]
    pub fn exclude<I: IntoIterator<Item = S>, S: Into<String>>(mut self, ids: I) -> Self {
        self.exclude.extend(ids.into_iter().map(Into::into));
        self
    }
    /// Only read-only operations: GET/HEAD, or an `x-mcp-annotations`
    /// `readOnlyHint` of true. Applies on top of [`Self::include`].
    #[must_use]
    pub const fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    fn base_url(&self) -> Result<String, String> {
        let setting = service_url_setting(&self.service);
        let url = self
            .url
            .clone()
            .or_else(|| env(&setting))
            .ok_or_else(|| format!("Set {setting} to reach the {} service.", self.service))?;
        Ok(url.trim_end_matches('/').to_owned())
    }

    async fn load(&self) -> Result<Vec<Tool>, String> {
        let base = self.base_url()?;
        let version = self
            .version
            .clone()
            .or_else(|| env("VERSION"))
            .unwrap_or_else(|| "v1".into());
        let unreachable = || {
            format!(
                "The {} service did not return its API description.",
                self.service
            )
        };
        let response = self
            .http
            .get(format!("{base}/api/{version}/openapi"))
            .timeout(OPENAPI_TIMEOUT)
            .send()
            .await
            .map_err(|_| unreachable())?;
        if !response.status().is_success() {
            return Err(unreachable());
        }
        let bytes = read_bounded(response, MAX_DOCUMENT_BYTES)
            .await
            .map_err(|()| unreachable())?;
        let mut body: Value = serde_json::from_slice(&bytes).map_err(|_| unreachable())?;
        let document = match body.get_mut("latest") {
            Some(latest) if latest.is_object() => latest.take(),
            _ => body,
        };
        Ok(self.tools_from(&document, &base))
    }

    fn tools_from(&self, document: &Value, base: &str) -> Vec<Tool> {
        let mut tools = Vec::new();
        let Some(paths) = document.get("paths").and_then(Value::as_object) else {
            return tools;
        };
        for (path, methods) in paths {
            if skipped_path(path) {
                continue;
            }
            let Some(methods) = methods.as_object() else {
                continue;
            };
            for (method, operation) in methods {
                if !METHODS.contains(&method.as_str()) {
                    continue;
                }
                if let Some(tool) = self.tool(document, base, path, method, operation) {
                    tools.push(tool);
                }
            }
        }
        tools
    }

    /// The allowlist/denylist, then the defaults no list can override: no
    /// service-only operations and only JSON request and success bodies.
    fn admits(&self, document: &Value, id: &String, operation: &Value) -> bool {
        if self.include.as_ref().is_some_and(|ids| !ids.contains(id)) || self.exclude.contains(id) {
            return false;
        }
        let json_request = operation
            .pointer("/requestBody/content")
            .and_then(Value::as_object)
            .is_none_or(|content| content.is_empty() || content.contains_key("application/json"));
        json_request && json_success(operation) && !service_only(document, operation)
    }

    fn tool(
        &self,
        document: &Value,
        base: &str,
        path: &str,
        method: &str,
        operation: &Value,
    ) -> Option<Tool> {
        let id = operation
            .get("operationId")
            .and_then(Value::as_str)
            .map_or_else(|| format!("{method}_{path}"), str::to_owned);
        if !self.admits(document, &id, operation) {
            return None;
        }
        let content = operation
            .pointer("/requestBody/content")
            .and_then(Value::as_object);
        let parameters = operation
            .get("parameters")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let group = |location: &str| {
            let entries: Vec<&Value> = parameters
                .iter()
                .filter(|p| p.get("in").and_then(Value::as_str) == Some(location))
                .collect();
            if entries.is_empty() {
                return None;
            }
            let mut properties = Map::new();
            let mut required = Vec::new();
            for entry in entries {
                let Some(name) = entry.get("name").and_then(Value::as_str) else {
                    continue;
                };
                let schema = entry
                    .get("schema")
                    .cloned()
                    .unwrap_or_else(|| json!({"type": "string"}));
                properties.insert(name.to_owned(), inline(&schema, document, &mut Vec::new()));
                if location == "path"
                    || entry.get("required").and_then(Value::as_bool) == Some(true)
                {
                    required.push(Value::String(name.to_owned()));
                }
            }
            Some(json!({"type": "object", "properties": properties, "required": required}))
        };
        let mut properties = Map::new();
        let mut required = Vec::new();
        if let Some(params) = group("path") {
            properties.insert("params".into(), params);
            required.push(Value::String("params".into()));
        }
        if let Some(query) = group("query") {
            properties.insert("query".into(), query);
        }
        if let Some(body) = content
            .and_then(|content| content.get("application/json"))
            .and_then(|media| media.get("schema"))
        {
            properties.insert("body".into(), inline(body, document, &mut Vec::new()));
            required.push(Value::String("body".into()));
        }
        let summary = operation
            .get("summary")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        let description = operation
            .get("description")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .or(summary)
            .map_or_else(
                || format!("{} {path} on {}", method.to_ascii_uppercase(), self.service),
                str::to_owned,
            );
        let annotations = operation
            .get("x-mcp-annotations")
            .and_then(|value| serde_json::from_value::<ToolAnnotations>(value.clone()).ok())
            .unwrap_or_else(|| method_annotations(method));
        if self.read_only && annotations.read_only_hint != Some(true) {
            return None;
        }
        let call = ServiceCall {
            http: self.http.clone(),
            base: base.to_owned(),
            method: method.to_ascii_uppercase(),
            path: path.to_owned(),
        };
        let mut tool = Tool::new(
            prefixed_tool_name(&self.service, &id),
            summary.unwrap_or_default(),
            description,
            json!({"type": "object", "properties": properties, "required": required}),
        )
        .annotations(annotations);
        tool = tool.handler(move |input, context| {
            let call = call.clone();
            async move { call.run(input, &context).await }
        });
        Some(tool)
    }
}
impl ToolSource for ServiceTools {
    fn kind(&self) -> ToolSourceKind {
        ToolSourceKind::Service
    }
    fn name(&self) -> &str {
        &self.service
    }
    fn list<'a>(
        &'a self,
        _: &'a ToolContext,
        refresh: bool,
    ) -> BoxFuture<'a, Result<Vec<Tool>, String>> {
        Box::pin(async move {
            if !refresh && let Some(tools) = self.cache.get() {
                return Ok(tools);
            }
            let tools = self.load().await?;
            self.cache.set(tools.clone());
            Ok(tools)
        })
    }
}

/// GET/HEAD read-only; DELETE destructive; PUT idempotent; anything else needs approval.
#[must_use]
pub fn method_annotations(method: &str) -> ToolAnnotations {
    let mut annotations = ToolAnnotations {
        read_only_hint: Some(false),
        ..ToolAnnotations::default()
    };
    match method.to_ascii_lowercase().as_str() {
        "get" | "head" => annotations.read_only_hint = Some(true),
        "delete" => annotations.destructive_hint = Some(true),
        "put" => annotations.idempotent_hint = Some(true),
        _ => {}
    }
    annotations
}

/// `access: internal` (`x-forklaunch-access`) or HMAC-secured operations are
/// service-to-service only; a user's forwarded token can never call them.
fn service_only(document: &Value, operation: &Value) -> bool {
    if operation.get("x-forklaunch-access").and_then(Value::as_str) == Some("internal") {
        return true;
    }
    let hmac_schemes: BTreeSet<&str> = document
        .pointer("/components/securitySchemes")
        .and_then(Value::as_object)
        .map(|schemes| {
            schemes
                .iter()
                .filter(|(name, scheme)| {
                    name.as_str() == "hmacAuth"
                        || scheme
                            .get("scheme")
                            .and_then(Value::as_str)
                            .is_some_and(|value| value.eq_ignore_ascii_case("hmac"))
                })
                .map(|(name, _)| name.as_str())
                .collect()
        })
        .unwrap_or_default();
    operation
        .get("security")
        .and_then(Value::as_array)
        .is_some_and(|requirements| {
            !requirements.is_empty()
                && requirements.iter().all(|requirement| {
                    requirement.as_object().is_some_and(|names| {
                        names
                            .keys()
                            .any(|name| name == "hmacAuth" || hmac_schemes.contains(name.as_str()))
                    })
                })
        })
}

/// Every success response either has no body or offers `application/json`.
/// Streams (SSE) and binary downloads cannot be returned to a model as JSON.
fn json_success(operation: &Value) -> bool {
    let Some(responses) = operation.get("responses").and_then(Value::as_object) else {
        return true;
    };
    responses
        .iter()
        .filter(|(status, _)| status.starts_with('2'))
        .all(|(_, response)| {
            response
                .get("content")
                .and_then(Value::as_object)
                .is_none_or(|content| {
                    content.is_empty()
                        || content
                            .keys()
                            .any(|media| media.starts_with("application/json"))
                })
        })
}

/// Health, docs, `OpenAPI`, compliance, MCP and agent routes are never tools.
#[must_use]
pub fn skipped_path(path: &str) -> bool {
    path.split('/').any(|segment| {
        matches!(
            segment,
            "openapi" | "docs" | "health" | "ready" | "compliance" | "agent" | "mcp"
        )
    })
}

/// Inlines local `#/components/schemas/X` references; cycles become plain objects.
fn inline(schema: &Value, document: &Value, seen: &mut Vec<String>) -> Value {
    match schema {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| inline(item, document, seen))
                .collect(),
        ),
        Value::Object(object) => {
            if let Some(key) = object
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(|reference| reference.strip_prefix("#/components/schemas/"))
            {
                if seen.iter().any(|name| name == key) {
                    return json!({"type": "object"});
                }
                let target = document
                    .pointer(&format!(
                        "/components/schemas/{}",
                        key.replace('~', "~0").replace('/', "~1")
                    ))
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                seen.push(key.to_owned());
                let resolved = inline(&target, document, seen);
                seen.pop();
                return resolved;
            }
            Value::Object(
                object
                    .iter()
                    .map(|(key, value)| (key.clone(), inline(value, document, seen)))
                    .collect(),
            )
        }
        other => other.clone(),
    }
}

#[derive(Clone)]
struct ServiceCall {
    http: Client,
    base: String,
    method: String,
    path: String,
}
impl ServiceCall {
    async fn run(&self, input: Value, context: &ToolContext) -> Result<Value, ToolError> {
        let empty = Map::new();
        let params = input
            .get("params")
            .and_then(Value::as_object)
            .unwrap_or(&empty);
        let mut filled = String::with_capacity(self.path.len());
        let mut rest = self.path.as_str();
        while let Some(start) = rest.find('{') {
            let Some(end) = rest[start..].find('}') else {
                break;
            };
            filled.push_str(&rest[..start]);
            let key = &rest[start + 1..start + end];
            let value = params
                .get(key)
                .filter(|value| !value.is_null())
                .ok_or_else(|| ToolError::invalid_input(format!("Missing {key}.")))?;
            let text = value
                .as_str()
                .map_or_else(|| value.to_string(), str::to_owned);
            filled.push_str(&utf8_percent_encode(&text, SEGMENT).to_string());
            rest = &rest[start + end + 1..];
        }
        filled.push_str(rest);
        let mut url = url::Url::parse(&format!("{}{filled}", self.base))
            .map_err(|_| ToolError::new("The service URL is invalid."))?;
        if let Some(query) = input.get("query").and_then(Value::as_object) {
            let mut pairs = url.query_pairs_mut();
            for (key, value) in query {
                match value {
                    Value::Null => {}
                    Value::String(text) => {
                        pairs.append_pair(key, text);
                    }
                    other => {
                        pairs.append_pair(key, &other.to_string());
                    }
                }
            }
        }
        if url.query() == Some("") {
            url.set_query(None);
        }
        let method = Method::from_bytes(self.method.as_bytes())
            .map_err(|_| ToolError::new("Unsupported method."))?;
        let mut request = self
            .http
            .request(method, url)
            .timeout(context.remaining().max(Duration::from_millis(1)));
        if let Some(authorization) = &context.authorization {
            request = request.header(reqwest::header::AUTHORIZATION, authorization);
        }
        if let Some(body) = input.get("body") {
            request = request.json(body);
        }
        let response = request.send().await.map_err(|error| {
            ToolError::new(if error.is_timeout() {
                "The service did not answer in time."
            } else {
                "The service could not be reached."
            })
        })?;
        let status = response.status();
        let bytes = read_bounded(response, MAX_SERVICE_RESPONSE_BYTES)
            .await
            .map_err(|()| ToolError::new("The service response was too large or interrupted."))?;
        let text = String::from_utf8_lossy(&bytes);
        if !status.is_success() {
            let detail: String = text.chars().take(500).collect();
            return Err(ToolError::new(format!(
                "The service answered {}: {detail}",
                status.as_u16()
            )));
        }
        Ok(serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(text.into_owned())))
    }
}

async fn read_bounded(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>, ()> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| ())? {
        if bytes.len() + chunk.len() > limit {
            return Err(());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// Tools of an external MCP server over streamable HTTP, named
/// `<server>__<tool>` and treated as open-world. Credentials are settings:
/// pass header values read from the environment.
///
/// ```ignore
/// McpServer::new("linear", env("LINEAR_MCP_URL"))
///     .header("authorization", env_bearer("LINEAR_MCP_TOKEN"));
/// ```
#[derive(Clone)]
pub struct McpServer {
    name: String,
    url: Option<String>,
    headers: Vec<(String, String)>,
    cache: Arc<Cache<Vec<Tool>>>,
}
impl std::fmt::Debug for McpServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpServer")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}
impl McpServer {
    #[must_use]
    pub fn new(name: impl Into<String>, url: Option<String>) -> Self {
        Self {
            name: name.into(),
            url: url.filter(|url| !url.is_empty()),
            headers: Vec::new(),
            cache: Arc::new(Cache::new()),
        }
    }
    /// Adds a header when the setting is present.
    #[must_use]
    pub fn header(mut self, name: impl Into<String>, value: Option<String>) -> Self {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            self.headers.push((name.into(), value));
        }
        self
    }

    fn options(&self, timeout: Duration) -> Result<McpClientOptions, String> {
        let url = self
            .url
            .clone()
            .ok_or_else(|| format!("Set the URL setting for the {} MCP server.", self.name))?;
        let mut options = McpClientOptions::new(url).timeout(timeout);
        for (name, value) in &self.headers {
            options = options.header(name.clone(), value.clone());
        }
        Ok(options)
    }

    async fn load(&self) -> Result<Vec<Tool>, String> {
        let client = McpClient::connect(self.options(MCP_LIST_TIMEOUT)?)
            .await
            .map_err(|error| error.message)?;
        let listed = client.list_tools().await;
        client.close().await;
        let listed = listed.map_err(|error| error.message)?;
        let open_world = ToolAnnotations {
            open_world_hint: Some(true),
            ..ToolAnnotations::default()
        };
        let server = Arc::new(self.clone());
        Ok(listed
            .into_iter()
            .map(|remote| {
                let annotations = remote
                    .annotations
                    .clone()
                    .and_then(|value| serde_json::from_value::<ToolAnnotations>(value).ok())
                    .unwrap_or_default()
                    .or(open_world.clone());
                let title = remote.title.clone().or_else(|| annotations.title.clone());
                let remote_name = remote.name.clone();
                let server = server.clone();
                Tool::new(
                    prefixed_tool_name(&self.name, &remote.name),
                    title.unwrap_or_default(),
                    remote
                        .description
                        .clone()
                        .filter(|d| !d.is_empty())
                        .unwrap_or_else(|| remote.name.clone()),
                    remote.input_schema,
                )
                .annotations(annotations)
                .handler(move |input, context| {
                    let server = server.clone();
                    let name = remote_name.clone();
                    async move { server.call(&name, input, &context).await }
                })
            })
            .collect())
    }

    async fn call(
        &self,
        name: &str,
        input: Value,
        context: &ToolContext,
    ) -> Result<Value, ToolError> {
        let options = self
            .options(context.remaining().max(Duration::from_millis(1)))
            .map_err(ToolError::new)?;
        let client = McpClient::connect(options).await?;
        let arguments = if input.is_object() { input } else { json!({}) };
        let result = client.call_tool(name, arguments).await;
        client.close().await;
        let result = result?;
        if result.is_error {
            let detail: String = result.content.to_string().chars().take(500).collect();
            return Err(ToolError::new(if detail.is_empty() {
                "The MCP tool failed.".to_owned()
            } else {
                detail
            }));
        }
        Ok(result.structured_content.unwrap_or(result.content))
    }
}
impl ToolSource for McpServer {
    fn kind(&self) -> ToolSourceKind {
        ToolSourceKind::Mcp
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn list<'a>(
        &'a self,
        _: &'a ToolContext,
        refresh: bool,
    ) -> BoxFuture<'a, Result<Vec<Tool>, String>> {
        Box::pin(async move {
            if !refresh && let Some(tools) = self.cache.get() {
                return Ok(tools);
            }
            let tools = self.load().await?;
            self.cache.set(tools.clone());
            Ok(tools)
        })
    }
}
