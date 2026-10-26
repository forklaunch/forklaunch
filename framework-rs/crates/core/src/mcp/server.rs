//! Streamable HTTP MCP transport using the official Rust SDK. Tool execution
//! stays inside the supplied HTTP router, preserving its validation, auth,
//! tenant binding, telemetry and handlers without making loopback requests.
use crate::mcp::{AllowAllAuth, McpAuthCallback, mcp_routes, mcp_tools};
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, HeaderName, HeaderValue, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use base64::Engine;
use forklaunch_core::contract::{BodyKind, ContractRegistry, RouteContract};
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
        ListToolsResult, PaginatedRequestParams, ResourceContents, ServerCapabilities,
        ServerConfig, Tool,
    },
    service::RequestContext,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde_json::{Map, Value};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tower::ServiceExt;

mod listener;
pub use listener::serve_http;
mod body_admission;
pub use body_admission::{McpBodyAdmission, McpBodyAdmissions, MissingToolBody};
mod manual;
mod multipart;
use manual::ManualHandler;
pub use manual::{ManualTool, ManualToolError};

#[derive(Clone)]
enum ToolBinding {
    Http(Box<RouteContract>, Option<Arc<body_admission::Admission>>),
    Manual(ManualHandler),
}

const URI_COMPONENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~')
    .remove(b'!')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

/// Transport options. Route authentication still applies independently to each
/// tool call. Default Host/Origin checks restrict access to local clients.
pub struct McpServerOptions {
    pub path: String,
    pub name: String,
    pub version: String,
    pub transport: StreamableHttpServerConfig,
    pub authorize: Arc<dyn McpAuthCallback + Send + Sync>,
    pub call_timeout: Duration,
    pub max_response_bytes: usize,
    /// Additional typed tools. Their handlers apply application authorization;
    /// the transport authorization callback still gates every request.
    pub manual_tools: Vec<ManualTool>,
    /// Explicit finite admission/default policies for HTTP tool bodies.
    pub body_admissions: McpBodyAdmissions,
}
impl McpServerOptions {
    #[must_use]
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            path: "/mcp".into(),
            name: name.into(),
            version: version.into(),
            transport: StreamableHttpServerConfig::default()
                .enforce_origin_validation()
                .with_legacy_session_mode(false)
                .with_json_response(true)
                .with_max_request_body_bytes(2 * 1024 * 1024),
            authorize: Arc::new(AllowAllAuth),
            call_timeout: Duration::from_secs(30),
            max_response_bytes: 2 * 1024 * 1024,
            manual_tools: Vec::new(),
            body_admissions: McpBodyAdmissions::default(),
        }
    }
}
#[derive(Clone)]
struct ContractServer {
    router: Router,
    tools: Arc<BTreeMap<String, (Tool, ToolBinding)>>,
    info: ServerConfig,
    timeout: Duration,
    max_bytes: usize,
}
fn validate_request_groups(
    registry: &ContractRegistry,
    options: &McpServerOptions,
) -> Result<(), String> {
    options
        .body_admissions
        .validate_routes(&mcp_routes(registry))?;
    for route in mcp_routes(registry) {
        for schema in [&route.request.query_schema, &route.request.headers_schema]
            .into_iter()
            .flatten()
        {
            super::checked_group(schema)?;
        }
    }
    Ok(())
}

/// Mounts MCP on the same service router. The dispatched clone excludes MCP,
/// so tools cannot recursively invoke the transport. No session credentials
/// are retained: every request must present its own transport/route credentials.
/// # Errors
/// Rejects invalid paths, bounds, schemas, duplicate tool names and endpoint collisions.
pub fn mount_http(
    router: Router,
    registry: &ContractRegistry,
    options: McpServerOptions,
) -> Result<Router, String> {
    let transport = http_transport(router.clone(), registry, options)?;
    Ok(router.merge(transport))
}

/// Creates an MCP-only router for a separate listener. Tool invocations still
/// dispatch through the supplied HTTP router, with the same route guards.
/// # Errors
/// Rejects invalid configuration, duplicate tools and endpoint collisions.
pub fn http_transport(
    router: Router,
    registry: &ContractRegistry,
    options: McpServerOptions,
) -> Result<Router, String> {
    if !options.path.starts_with('/')
        || options.path == "/"
        || options.path.ends_with('/')
        || options.path.contains(['{', '}', ':', '?', '#', '*', '%'])
        || options.path.contains("//")
        || options
            .path
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
        || options.call_timeout.is_zero()
        || options.max_response_bytes == 0
    {
        return Err("Invalid MCP endpoint or resource bounds".into());
    }
    if registry
        .routes()
        .iter()
        .any(|r| r.path == options.path || r.openapi_path == options.path)
    {
        return Err("MCP endpoint conflicts with an HTTP route".into());
    }
    validate_request_groups(registry, &options)?;
    let mut tools = BTreeMap::new();
    for (definition, route) in mcp_tools(registry).into_iter().zip(mcp_routes(registry)) {
        let mut tool: Tool =
            serde_json::from_str(&definition.to_json()).map_err(|_| "Invalid MCP tool schema")?;
        let admission = options.body_admissions.project(&route, &mut tool)?;
        // MCP structured outputs must be objects. Array/scalar HTTP bodies are
        // represented as text rather than claiming an invalid output schema.
        if tool
            .output_schema
            .as_ref()
            .is_some_and(|s| s.get("type").and_then(Value::as_str) != Some("object"))
        {
            tool.output_schema = None;
        }
        if route
            .responses
            .iter()
            .filter_map(|r| r.body.as_ref())
            .any(|body| body.kind != BodyKind::Json)
        {
            tool.output_schema = None;
        }
        if tools
            .insert(
                definition.name,
                (tool, ToolBinding::Http(Box::new(route), admission)),
            )
            .is_some()
        {
            return Err("Duplicate MCP tool name".into());
        }
    }
    for manual in options.manual_tools {
        let name = manual.definition.name.to_string();
        if tools
            .insert(
                name,
                (manual.definition, ToolBinding::Manual(manual.handler)),
            )
            .is_some()
        {
            return Err("Duplicate MCP tool name".into());
        }
    }
    let handler = ContractServer {
        router,
        tools: Arc::new(tools),
        info: ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(options.name, options.version)),
        timeout: options.call_timeout,
        max_bytes: options.max_response_bytes,
    };
    let service = StreamableHttpService::new(
        move || Ok(handler.clone()),
        Arc::new(LocalSessionManager::default()),
        options.transport,
    );
    let authorize = options.authorize;
    let transport = Router::new()
        .route_service(&options.path, service)
        .layer(middleware::from_fn(
            move |request: Request<Body>, next: Next| {
                let authorize = Arc::clone(&authorize);
                async move {
                    let token = request
                        .headers()
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.strip_prefix("Bearer "));
                    if !authorize.authorize(token) {
                        return (StatusCode::UNAUTHORIZED, "MCP authorization required")
                            .into_response();
                    }
                    next.run(request).await
                }
            },
        ));
    Ok(transport)
}
impl ServerHandler for ContractServer {
    fn get_info(&self) -> ServerConfig {
        self.info.clone()
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools.get(name).map(|(tool, _)| tool.clone())
    }
    fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, ErrorData>> + Send {
        std::future::ready(Ok(ListToolsResult::with_all_items(
            self.tools.values().map(|(tool, _)| tool.clone()).collect(),
        )))
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let (_, binding) = self
            .tools
            .get(request.name.as_ref())
            .ok_or_else(|| ErrorData::invalid_params("Unknown tool", None))?;
        let mut args = request.arguments.unwrap_or_default();
        let headers = context
            .extensions
            .get::<axum::http::request::Parts>()
            .map(|parts| &parts.headers);
        let (route, omit_body) = match binding {
            ToolBinding::Http(route, admission) => {
                let omit = admission
                    .as_ref()
                    .map(|admission| admission.apply(&mut args))
                    .transpose()
                    .map_err(body_admission::AdmissionFailure::into_rpc)?
                    .unwrap_or(false);
                (route, omit)
            }
            ToolBinding::Manual(handler) => {
                return manual::execute(
                    handler,
                    args,
                    headers.cloned().unwrap_or_default(),
                    self.timeout,
                    self.max_bytes,
                )
                .await;
            }
        };
        let http_request = build_request_with_body_omission(route, &args, headers, omit_body)
            .map_err(|message| ErrorData::invalid_params(message, None))?;
        let response = tokio::time::timeout(self.timeout, async {
            let response: Response = self
                .router
                .clone()
                .oneshot(http_request)
                .await
                .expect("Axum router is infallible");
            let status = response.status();
            let mime = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_owned();
            let body = to_bytes(response.into_body(), self.max_bytes)
                .await
                .map_err(|_| "HTTP response exceeds MCP limit")?;
            Ok::<_, &str>((status, mime, body))
        })
        .await;
        let (status, mime, body) = match response {
            Ok(Ok(value)) => value,
            Ok(Err(message)) => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(message)]).into());
            }
            Err(_) => {
                return Ok(
                    CallToolResult::error(vec![ContentBlock::text("HTTP tool timed out")]).into(),
                );
            }
        };
        if status.is_success() && mime.split(';').next() == Some("application/octet-stream") {
            return Ok(CallToolResult::success(vec![ContentBlock::resource(
                ResourceContents::blob(
                    base64::engine::general_purpose::STANDARD.encode(&body),
                    format!("forklaunch://tool/{}", route.operation_name),
                )
                .with_mime_type(mime),
            )])
            .into());
        }
        let text = String::from_utf8_lossy(&body).into_owned();
        let mut result = if status.is_success() {
            CallToolResult::success(vec![ContentBlock::text(&text)])
        } else {
            CallToolResult::error(vec![ContentBlock::text(format!("HTTP {status}: {text}"))])
        };
        if status.is_success()
            && mime.split(';').next() == Some("application/json")
            && let Ok(value @ Value::Object(_)) = serde_json::from_slice(&body)
        {
            result.structured_content = Some(value);
        }
        Ok(result.into())
    }
}
fn scalar(value: &Value) -> Result<String, String> {
    match value {
        Value::String(v) => Ok(v.clone()),
        Value::Bool(_) | Value::Number(_) => Ok(value.to_string()),
        _ => Err("Expected a string, number or boolean".into()),
    }
}
#[cfg(test)]
fn build_request(
    route: &RouteContract,
    args: &Map<String, Value>,
    transport_headers: Option<&HeaderMap>,
) -> Result<Request<Body>, String> {
    build_request_with_body_omission(route, args, transport_headers, false)
}

fn copy_trace_parent(headers: &mut HeaderMap, transport_headers: Option<&HeaderMap>) {
    // Preserve the actual envelope's W3C parent as one pair. Never combine its
    // parent with a tool-supplied tracestate. Baggage is deliberately not copied.
    if let Some(parent) = transport_headers.and_then(|h| h.get("traceparent")) {
        headers.insert("traceparent", parent.clone());
        headers.remove("tracestate");
        if let Some(state) = transport_headers.and_then(|h| h.get("tracestate")) {
            headers.insert("tracestate", state.clone());
        }
    }
}

fn build_request_with_body_omission(
    route: &RouteContract,
    args: &Map<String, Value>,
    transport_headers: Option<&HeaderMap>,
    omit_body: bool,
) -> Result<Request<Body>, String> {
    let mut path = String::new();
    for part in route.path.split('/') {
        if !path.is_empty() || route.path.starts_with('/') {
            path.push('/');
        }
        if part.is_empty() {
            continue;
        }
        let name = part
            .strip_prefix(':')
            .or_else(|| part.strip_prefix('{').and_then(|s| s.strip_suffix('}')));
        if let Some(name) = name {
            let value = args
                .get("params")
                .and_then(|p| p.get(name))
                .ok_or_else(|| format!("Missing path parameter {name}"))?;
            path.push_str(
                &percent_encoding::utf8_percent_encode(&scalar(value)?, URI_COMPONENT).to_string(),
            );
        } else {
            path.push_str(part);
        }
    }
    // split('/') has a leading empty component; normalize only that separator.
    if path.starts_with("//") {
        path.remove(0);
    }
    if let Some(query) = args.get("query") {
        let query = query.as_object().ok_or("query must be an object")?;
        let mut encoded = form_urlencoded::Serializer::new(String::new());
        for (name, value) in query {
            // TypeScript safeStringify preserves strings and serializes JSON
            // composites into one query value, including empty arrays.
            let value = value
                .as_str()
                .map_or_else(|| value.to_string(), str::to_owned);
            encoded.append_pair(name, &value);
        }
        let query = encoded.finish();
        if !query.is_empty() {
            path.push('?');
            path.push_str(&query);
        }
    }
    let mut headers = HeaderMap::new();
    // The MCP envelope's Authorization is useful for JWT routes. Signed HTTP
    // credentials may instead be supplied explicitly in the tool arguments.
    if let Some(authorization) = transport_headers.and_then(|h| h.get("authorization")) {
        headers.insert("authorization", authorization.clone());
    }
    if let Some(input) = args.get("headers") {
        for (name, value) in input.as_object().ok_or("headers must be an object")? {
            let name = HeaderName::try_from(name.as_str()).map_err(|_| "Invalid header name")?;
            if matches!(
                name.as_str(),
                "host" | "content-length" | "transfer-encoding" | "connection" | "upgrade"
            ) {
                return Err("Unsupported HTTP transport header".into());
            }
            let value =
                HeaderValue::try_from(scalar(value)?).map_err(|_| "Invalid header value")?;
            headers.insert(name, value);
        }
    }
    // Keep the transport trace attached to the internally dispatched operation.
    // Explicit argument correlation remains supported when the envelope has none.
    if let Some(correlation) = transport_headers.and_then(|h| h.get("x-correlation-id")) {
        headers.insert("x-correlation-id", correlation.clone());
    }
    copy_trace_parent(&mut headers, transport_headers);
    // Browser credentials must retain the actual envelope's Origin and CSRF
    // proof. Tool arguments cannot overwrite that trust boundary.
    if matches!(
        route.auth,
        Some(forklaunch_core::contract::AuthConfig::Cookie { .. })
    ) && let Some(transport) = transport_headers
    {
        for name in ["cookie", "origin", "x-csrf-token"] {
            headers.remove(name);
            for value in transport.get_all(name) {
                headers.append(name, value.clone());
            }
        }
    }
    let body = encode_body(route, args, &mut headers, omit_body)?;
    // Actual fetch frames nonempty encoded bodies, and sends zero length for
    // POST/PUT/PATCH. Empty DELETE/OPTIONS bodies remain unframed. Derive it only from encoded
    // bytes; tool-supplied transport headers remain forbidden above.
    if !body.is_empty()
        || matches!(
            route.method,
            forklaunch_core::contract::HttpMethod::Post
                | forklaunch_core::contract::HttpMethod::Put
                | forklaunch_core::contract::HttpMethod::Patch
        )
    {
        headers.insert(
            "content-length",
            HeaderValue::from_str(&body.len().to_string())
                .map_err(|_| "Invalid encoded body length")?,
        );
    }
    let mut request = Request::builder()
        .method(route.method.as_str())
        .uri(path)
        .body(Body::from(body))
        .map_err(|_| "Invalid HTTP tool request")?;
    *request.headers_mut() = headers;
    Ok(request)
}

fn encode_body(
    route: &RouteContract,
    args: &Map<String, Value>,
    headers: &mut HeaderMap,
    omit_body: bool,
) -> Result<Vec<u8>, String> {
    let body = if let Some(contract) = &route.request.body {
        let body = args.get("body");
        if body.is_none()
            && (!omit_body || !matches!(contract.kind, BodyKind::Json | BodyKind::Text))
        {
            return Err("Missing body".into());
        }
        let default_mime = match contract.kind {
            BodyKind::Text => "text/plain",
            BodyKind::Binary | BodyKind::File => "application/octet-stream",
            BodyKind::Multipart => "multipart/form-data",
            BodyKind::UrlEncoded => "application/x-www-form-urlencoded",
            BodyKind::ServerSentEvent => "text/event-stream",
            BodyKind::Json | BodyKind::Unknown => "application/json",
        };
        let mime = args
            .get("contentType")
            .and_then(Value::as_str)
            .unwrap_or(contract.content_type.as_deref().unwrap_or(default_mime));
        if mime != contract.content_type.as_deref().unwrap_or(default_mime) {
            return Err("Content type does not match route contract".into());
        }
        headers.insert(
            "content-type",
            HeaderValue::try_from(mime).map_err(|_| "Invalid content type")?,
        );
        let Some(body) = body else {
            return Ok(Vec::new());
        };
        match contract.kind {
            // MCP arguments are typed values. JSON strings need their JSON quotes;
            // callers that own signed wire bytes use an explicit Text/File contract.
            BodyKind::Json => serde_json::to_vec(body).map_err(|_| "Invalid JSON body")?,
            BodyKind::Unknown => match body.as_str() {
                Some(value) => value.as_bytes().to_vec(),
                None => serde_json::to_vec(body).map_err(|_| "Invalid JSON body")?,
            },
            BodyKind::UrlEncoded => {
                let mut encoded = form_urlencoded::Serializer::new(String::new());
                for (name, value) in body.as_object().ok_or("Form body must be an object")? {
                    let value = value
                        .as_str()
                        .map_or_else(|| value.to_string(), str::to_owned);
                    encoded.append_pair(name, &value);
                }
                encoded.finish().into_bytes()
            }
            BodyKind::Multipart => {
                let (body, boundary) = multipart::encode(body)?;
                headers.insert(
                    "content-type",
                    HeaderValue::try_from(format!("multipart/form-data; boundary={boundary}"))
                        .map_err(|_| "Invalid multipart boundary")?,
                );
                body
            }
            BodyKind::Binary | BodyKind::File => body
                .as_str()
                .map_or_else(|| body.to_string(), str::to_owned)
                .into_bytes(),
            BodyKind::ServerSentEvent => return Err("SSE request bodies are not supported".into()),
            BodyKind::Text => body
                .as_str()
                .ok_or("Text body must be a string")?
                .as_bytes()
                .to_vec(),
        }
    } else {
        Vec::new()
    };
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forklaunch_core::contract::{FieldContract, RouteContract};
    #[test]
    fn correlation_preserves_transport_precedence_and_argument_fallback() {
        let route = RouteContract::get("Read", "Read", "/read");
        for (transport_value, argument_value, expected) in [
            (Some("transport"), None, Some("transport")),
            (Some("transport"), Some("argument"), Some("transport")),
            (None, Some("argument"), Some("argument")),
            (None, None, None),
        ] {
            let mut transport = HeaderMap::new();
            if let Some(value) = transport_value {
                transport.insert("x-correlation-id", HeaderValue::from_static(value));
            }
            let args = argument_value.map_or_else(
                || serde_json::json!({}),
                |value| serde_json::json!({"headers":{"x-correlation-id":value}}),
            );
            let request =
                build_request(&route, args.as_object().unwrap(), Some(&transport)).unwrap();
            assert_eq!(
                request
                    .headers()
                    .get("x-correlation-id")
                    .map(|value| value.to_str().unwrap()),
                expected
            );
        }
    }
    #[test]
    fn malformed_group_metadata_rejects_mcp_mount() {
        let mut route = RouteContract::get("bad", "bad", "/bad");
        route.request.query_schema = Some(
            forklaunch_core::contract::SchemaRef::named("Bad")
                .with_json_schema(r#"{"type":"array"}"#),
        );
        let mut registry = ContractRegistry::new();
        registry.register(route);
        assert!(
            http_transport(
                Router::new(),
                &registry,
                McpServerOptions::new("group-test", "1.0.0")
            )
            .is_err()
        );
    }

    #[test]
    fn w3c_trace_parent_and_state_retain_transport_precedence() {
        let route = RouteContract::get("read", "read", "/read");
        let parent = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        for state in [None, Some("vendor=opaque")] {
            let mut transport = HeaderMap::new();
            transport.insert("traceparent", HeaderValue::from_static(parent));
            transport.insert("baggage", HeaderValue::from_static("private=never-forward"));
            if let Some(state) = state {
                transport.insert("tracestate", HeaderValue::from_static(state));
            }
            let args = serde_json::json!({"headers":{"traceparent":"argument-parent","tracestate":"argument=state"}});
            let request =
                build_request(&route, args.as_object().unwrap(), Some(&transport)).unwrap();
            assert_eq!(request.headers()["traceparent"], parent);
            assert_eq!(
                request
                    .headers()
                    .get("tracestate")
                    .map(|v| v.to_str().unwrap()),
                state
            );
            assert!(!request.headers().contains_key("baggage"));
        }
    }

    #[test]
    fn cookie_tools_preserve_transport_security_headers() {
        let route = RouteContract::post("Write", "Write", "/write")
            .with_auth(forklaunch_core::contract::AuthConfig::cookie("session"));
        let args = serde_json::json!({"headers": {"origin":"https://spoof.test", "cookie":"session=spoof", "x-csrf-token":"spoof"}});
        let mut transport = HeaderMap::new();
        transport.insert("origin", HeaderValue::from_static("https://real.test"));
        transport.insert("cookie", HeaderValue::from_static("session=real"));
        let request = build_request(&route, args.as_object().unwrap(), Some(&transport)).unwrap();
        assert_eq!(request.headers()["origin"], "https://real.test");
        assert_eq!(request.headers()["cookie"], "session=real");
        assert!(!request.headers().contains_key("x-csrf-token"));
    }
    #[test]
    fn raw_text_json_media_type_preserves_signed_bytes() {
        let raw = "{\n  \"id\": \"evt_💬\"\n}";
        let mut route = RouteContract::post("Webhook", "Webhook", "/webhook");
        route.request.body = Some(forklaunch_core::contract::BodyContract {
            kind: BodyKind::Text,
            schema: forklaunch_core::contract::SchemaRef::named("String"),
            content_type: Some("application/json".into()),
        });
        let args = serde_json::json!({"body":raw});
        let mut headers = HeaderMap::new();
        assert_eq!(
            encode_body(&route, args.as_object().unwrap(), &mut headers, false).unwrap(),
            raw.as_bytes()
        );
        assert_eq!(headers["content-type"], "application/json");
        let invalid = serde_json::json!({"body":{"id":"evt"}});
        assert!(encode_body(&route, invalid.as_object().unwrap(), &mut headers, false).is_err());
        route.request.body.as_mut().unwrap().kind = BodyKind::Json;
        assert_eq!(
            encode_body(&route, invalid.as_object().unwrap(), &mut headers, false).unwrap(),
            br#"{"id":"evt"}"#
        );
    }
    #[test]
    fn native_query_transport_preserves_json_values_and_single_escaping() {
        let route = RouteContract::get("Read Query", "Read query", "/query/:id")
            .with_param(FieldContract::required("id", "String"));
        let args = serde_json::json!({"params":{"id":"a/b + 水"},"query":{"ids":["a,b","水"],"empty":[],"filter":{"active":false}}});
        let request = build_request(&route, args.as_object().unwrap(), None).unwrap();
        let values: BTreeMap<_, _> =
            form_urlencoded::parse(request.uri().query().unwrap().as_bytes())
                .into_owned()
                .collect();
        assert_eq!(values["ids"], r#"["a,b","水"]"#);
        assert_eq!(values["empty"], "[]");
        assert_eq!(values["filter"], r#"{"active":false}"#);
        assert_eq!(request.uri().path(), "/query/a%2Fb%20%2B%20%E6%B0%B4");
        if let (Ok(input), Ok(output)) = (
            std::env::var("FORKLAUNCH_TEST_QUERY_TRANSPORT_INPUT"),
            std::env::var("FORKLAUNCH_TEST_QUERY_TRANSPORT_OUTPUT"),
        ) {
            let cases: Vec<Value> = serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
            let targets: Vec<_> = cases
                .iter()
                .map(|args| {
                    build_request(&route, args.as_object().unwrap(), None)
                        .unwrap()
                        .uri()
                        .to_string()
                })
                .collect();
            std::fs::write(
                std::path::Path::new(&output).join("mcp-targets.json"),
                serde_json::to_vec(&targets).unwrap(),
            )
            .unwrap();
        }
    }
}

#[cfg(test)]
#[path = "server/body_tests.rs"]
mod body_tests;
