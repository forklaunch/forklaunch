use super::*;
use crate::mcp::server::{McpServerOptions, mount_http};
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    routing::get,
};
use forklaunch_core::contract::{ContractRegistry, RouteContract};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use tower::ServiceExt;

#[derive(Deserialize, Serialize, schemars::JsonSchema, garde::Validate)]
#[serde(deny_unknown_fields)]
struct Input {
    #[garde(length(min = 1, max = 8))]
    #[schemars(length(min = 1, max = 8))]
    name: String,
}
#[derive(Deserialize, Serialize, schemars::JsonSchema, garde::Validate)]
#[serde(transparent)]
struct ScalarInput(#[garde(skip)] String);

struct BadOutput;
impl Serialize for BadOutput {
    fn serialize<S: serde::Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("sensitive provider payload"))
    }
}

struct TokenAuth;
impl crate::mcp::McpAuthCallback for TokenAuth {
    fn authorize(&self, token: Option<&str>) -> bool {
        token == Some("fixture-token")
    }
}
fn tool(name: &str, count: Arc<AtomicUsize>) -> ManualTool {
    ManualTool::typed::<Input, Value, _, _>(name, "Typed greeting", move |input, headers| {
        let count = count.clone();
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            if headers
                .get("x-tenant")
                .and_then(|value| value.to_str().ok())
                != Some("tenant-a")
            {
                return Err(ManualToolError::new("Tenant denied"));
            }
            match input.name.as_str() {
                "slow" => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                "error" => return Err(ManualToolError::new("Public failure")),
                "large" => return Ok(json!({"message":"x".repeat(128)})),
                "zero" => return Ok(json!(0)),
                "false" => return Ok(json!(false)),
                "array" => return Ok(json!([false, 0, null])),
                _ => {}
            }
            Ok(json!({"hello":input.name}))
        }
    })
    .unwrap()
}
async fn rpc(
    app: &Router,
    method: &str,
    params: Value,
    authorized: bool,
    tenant: &str,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("host", "localhost")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-11-25")
        .header("x-tenant", tenant);
    if authorized {
        request = request.header("authorization", "Bearer fixture-token");
    }
    let response = app
        .clone()
        .oneshot(
            request
                .body(Body::from(
                    json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 65536).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}
fn app(count: Arc<AtomicUsize>, timeout: Duration, bytes: usize) -> Router {
    let mut registry = ContractRegistry::new();
    registry.register(RouteContract::get("Hello", "HTTP hello", "/hello"));
    let mut options = McpServerOptions::new("manual-fixture", "1");
    options.authorize = Arc::new(TokenAuth);
    options.call_timeout = timeout;
    options.max_response_bytes = bytes;
    options.manual_tools.push(tool("greet", count));
    mount_http(
        Router::new().route("/hello", get(|| async { axum::Json(json!({"http":true})) })),
        &registry,
        options,
    )
    .unwrap()
}
#[tokio::test]
async fn manual_and_http_tools_share_transport_but_keep_per_call_guards_and_validation() {
    let count = Arc::new(AtomicUsize::new(0));
    let app = app(count.clone(), Duration::from_secs(1), 4096);
    let (status, _) = rpc(&app, "tools/list", json!({}), false, "tenant-a").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (_, initialized) = rpc(&app, "initialize", json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"fixture","version":"1"}}), true, "tenant-a").await;
    assert_eq!(
        initialized["result"]["serverInfo"]["name"],
        "manual-fixture"
    );
    let (_, listed) = rpc(&app, "tools/list", json!({}), true, "tenant-a").await;
    let tools = listed["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 2);
    assert!(tools.iter().any(|tool| tool["name"] == "hello"));
    assert!(tools.iter().any(|tool| tool["name"] == "greet"
        && tool["inputSchema"]["properties"]["name"]["type"] == "string"));
    let (_, http) = rpc(
        &app,
        "tools/call",
        json!({"name":"hello","arguments":{}}),
        true,
        "tenant-a",
    )
    .await;
    assert_eq!(http["result"]["structuredContent"], json!({"http":true}));
    for arguments in [
        json!({}),
        json!({"name":5}),
        json!({"name":""}),
        json!({"name":"too-long-name"}),
        json!({"name":"Ada","headers":{"x-tenant":"tenant-a"}}),
    ] {
        let (_, result) = rpc(
            &app,
            "tools/call",
            json!({"name":"greet","arguments":arguments}),
            true,
            "tenant-a",
        )
        .await;
        assert_eq!(result["error"]["code"], -32602, "{result}");
        assert!(!result.to_string().contains("too-long-name"));
    }
    assert_eq!(count.load(Ordering::SeqCst), 0);
    let (_, result) = rpc(
        &app,
        "tools/call",
        json!({"name":"greet","arguments":{"name":"Ada"}}),
        true,
        "tenant-a",
    )
    .await;
    assert_eq!(
        result["result"]["structuredContent"],
        json!({"hello":"Ada"})
    );
    let (_, result) = rpc(
        &app,
        "tools/call",
        json!({"name":"greet","arguments":{"name":"Ada"}}),
        true,
        "tenant-b",
    )
    .await;
    assert_eq!(result["result"]["isError"], true);
    assert_eq!(result["result"]["content"][0]["text"], "Tenant denied");
    let (_, missing) = rpc(
        &app,
        "tools/call",
        json!({"name":"missing","arguments":{}}),
        true,
        "tenant-a",
    )
    .await;
    assert_eq!(missing["error"]["code"], -32602);
}
#[tokio::test]
async fn manual_tools_bound_execution_and_response_sizes_and_preserve_scalar_values() {
    let app = app(Arc::new(AtomicUsize::new(0)), Duration::from_millis(10), 64);
    for (name, message) in [
        ("slow", "Manual tool timed out"),
        ("large", "Manual tool response exceeds MCP limit"),
        ("error", "Public failure"),
    ] {
        let (_, result) = rpc(
            &app,
            "tools/call",
            json!({"name":"greet","arguments":{"name":name}}),
            true,
            "tenant-a",
        )
        .await;
        assert_eq!(result["result"]["isError"], true, "{result}");
        assert_eq!(result["result"]["content"][0]["text"], message);
    }
    for (name, value) in [
        ("zero", json!(0)),
        ("false", json!(false)),
        ("array", json!([false, 0, null])),
    ] {
        let (_, result) = rpc(
            &app,
            "tools/call",
            json!({"name":"greet","arguments":{"name":name}}),
            true,
            "tenant-a",
        )
        .await;
        assert_eq!(result["result"]["isError"], false);
        assert_eq!(
            serde_json::from_str::<Value>(result["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap(),
            value
        );
        assert!(result["result"].get("structuredContent").is_none());
    }
}
#[test]
fn manual_tools_reject_invalid_names_scalar_inputs_and_name_collisions() {
    assert!(
        ManualTool::typed::<Input, Value, _, _>("", "", |_, _| async { Ok(Value::Null) }).is_err()
    );
    assert!(
        ManualTool::typed::<ScalarInput, Value, _, _>("scalar", "", |_, _| async {
            Ok(Value::Null)
        })
        .is_err()
    );
    let count = Arc::new(AtomicUsize::new(0));
    let mut registry = ContractRegistry::new();
    registry.register(RouteContract::get("Greet", "Existing", "/greet"));
    let mut options = McpServerOptions::new("fixture", "1");
    options.manual_tools.push(tool("greet", count.clone()));
    assert!(mount_http(Router::new(), &registry, options).is_err());
    let mut options = McpServerOptions::new("fixture", "1");
    options.manual_tools.push(tool("greet", count.clone()));
    options.manual_tools.push(tool("greet", count));
    assert!(mount_http(Router::new(), &ContractRegistry::new(), options).is_err());
}

#[tokio::test]
async fn response_serialization_failures_do_not_disclose_serializer_details() {
    let manual = ManualTool::typed::<Input, BadOutput, _, _>("bad-output", "", |_, _| async {
        Ok(BadOutput)
    })
    .unwrap();
    let mut options = McpServerOptions::new("fixture", "1");
    options.manual_tools.push(manual);
    let app = mount_http(Router::new(), &ContractRegistry::new(), options).unwrap();
    let (_, result) = rpc(
        &app,
        "tools/call",
        json!({"name":"bad-output","arguments":{"name":"Ada"}}),
        true,
        "tenant-a",
    )
    .await;
    assert_eq!(result["result"]["isError"], true);
    assert_eq!(
        result["result"]["content"][0]["text"],
        "Tool response serialization failed"
    );
    assert!(!result.to_string().contains("sensitive provider payload"));
}
