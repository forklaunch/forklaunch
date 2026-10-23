#![cfg(feature = "agent")]
//! Agent routes end to end: contract endpoints with the app's session policy,
//! a framework service as a tool source (`OpenAPI` over the main port) and the
//! framework's own MCP server as an external MCP source.
use axum::http::request::Parts;
use forklaunch_axum::agent::mount_agent;
use forklaunch_axum::native::{
    ContractRouter, EndpointError, EndpointPolicy, SessionPolicy, Shutdown, StreamOptions,
    with_openapi,
};
use forklaunch_axum::session::AuthenticatedSession;
use forklaunch_core::agent::{
    Agent, BoxFuture, McpServer, ModelBackend, ModelChunks, ModelError, Tool, contract,
    service_tools,
};
use forklaunch_core::auth::{AuthError, Principal};
use forklaunch_core::contract::endpoint::{Endpoint, JsonBody, NoBody};
use forklaunch_core::contract::{
    AccessLevel, ContractRegistry, HttpMethod, RouteContract, ServiceContract,
};
use forklaunch_core::mcp::McpAuthCallback;
use forklaunch_core::mcp::client::{McpClient, McpClientOptions};
use forklaunch_core::mcp::server::{McpServerOptions, mount_http};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct Note {
    id: String,
    title: String,
}
#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct Status {
    ok: bool,
}

/// Admits only the caller's bearer token, so a passing call proves forwarding.
#[derive(Clone)]
struct BearerPolicy;
#[allow(clippy::unused_async_trait_impl)]
impl EndpointPolicy<()> for BearerPolicy {
    type Context = ();
    async fn authorize(
        &self,
        parts: &mut Parts,
        (): &(),
        _: &RouteContract,
    ) -> Result<(), EndpointError> {
        if parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            == Some("Bearer user-token")
        {
            Ok(())
        } else {
            Err(EndpointError::Unauthorized)
        }
    }
}

struct Gate;
impl McpAuthCallback for Gate {
    fn authorize(&self, token: Option<&str>) -> bool {
        token == Some("mcp-secret")
    }
}

async fn serve(app: axum::Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    base
}

/// A framework service with an authenticated list route, a public status
/// route, its `OpenAPI` document and its MCP transport.
async fn notes_service() -> String {
    let list = Endpoint::<NoBody, JsonBody<Vec<Note>>>::new(
        "List Notes",
        "List notes",
        HttpMethod::Get,
        "/notes",
        200,
    )
    .unwrap()
    .with_access(AccessLevel::Authenticated);
    let status = Endpoint::<NoBody, JsonBody<Status>>::new(
        "Status",
        "Status",
        HttpMethod::Get,
        "/status",
        200,
    )
    .unwrap();
    let mut registry = ContractRegistry::for_service(ServiceContract::new("notes", "1.0.0"));
    registry.register(list.contract());
    registry.register(status.contract());
    let router = ContractRouter::<()>::new(registry.clone())
        .bind_authorized(list, BearerPolicy, |_, (), ()| async {
            Ok(vec![Note {
                id: "n1".into(),
                title: "Groceries".into(),
            }])
        })
        .unwrap()
        .bind(status, |_, ()| async { Ok(Status { ok: true }) })
        .unwrap()
        .finish()
        .unwrap();
    // A service-to-service route is published in the API description but must
    // never become an agent tool (a user's token cannot call it).
    let mut documented = registry.clone();
    documented.register(
        Endpoint::<JsonBody<Status>, JsonBody<Status>>::new(
            "Sync Notes",
            "Sync notes from a worker",
            HttpMethod::Post,
            "/internal/notes/sync",
            202,
        )
        .unwrap()
        .with_access(AccessLevel::Internal)
        .contract(),
    );
    let router = with_openapi(router, &documented, "/openapi.json", "v1").unwrap();
    let mut options = McpServerOptions::new("notes", "1.0.0");
    options.authorize = Arc::new(Gate);
    serve(mount_http(router, &registry, options).unwrap()).await
}

#[derive(Default)]
struct FakeModel {
    responses: Mutex<VecDeque<Value>>,
    requests: Mutex<Vec<Value>>,
}
impl ModelBackend for FakeModel {
    fn complete(&self, request: Value) -> BoxFuture<'_, Result<Value, ModelError>> {
        self.requests.lock().unwrap().push(request);
        let next = self.responses.lock().unwrap().pop_front();
        // `{"fail": <status>}` scripts a gateway refusal.
        let result = match next {
            None => Err(ModelError::status(500)),
            Some(value) => match value.get("fail").and_then(Value::as_u64) {
                Some(status) => Err(ModelError::status(u16::try_from(status).unwrap())),
                None => Ok(value),
            },
        };
        Box::pin(async move { result })
    }
    fn stream(&self, _: Value) -> BoxFuture<'_, Result<Box<dyn ModelChunks>, ModelError>> {
        Box::pin(async { Err(ModelError::status(501)) })
    }
}
fn answer(text: &str) -> Value {
    json!({"choices": [{"index": 0, "message": {"role": "assistant", "content": text}, "finish_reason": "stop"}]})
}
fn tool_call(id: &str, name: &str) -> Value {
    json!({"choices": [{"index": 0, "message": {"role": "assistant", "content": null, "tool_calls": [
        {"id": id, "type": "function", "function": {"name": name, "arguments": "{}"}}
    ]}, "finish_reason": "tool_calls"}]})
}

fn session_policy() -> SessionPolicy {
    SessionPolicy::new(
        Arc::new(|request, _| {
            Box::pin(async move {
                if request.headers.get("authorization").map(String::as_str)
                    == Some("Bearer user-token")
                {
                    Ok(AuthenticatedSession::new(
                        Principal::new("user-1"),
                        json!({}),
                    ))
                } else {
                    Err(AuthError::MissingCredentials)
                }
            })
        }),
        Arc::new(|_, _| Ok(())),
    )
}

async fn agent_service(notes: &str, model: Arc<FakeModel>) -> String {
    let agent = Agent::builder("assistant")
        .instructions("Help with notes.")
        .model("luna")
        .streaming(false)
        .continuation_secret("agent-secret")
        .model_backend(model)
        .tool(
            Tool::new(
                "current_time",
                "Current time",
                "The current UTC time.",
                json!({"type": "object"}),
            )
            .read_only()
            .handler(|_, _| async { Ok(json!({"now": "2026-10-20T00:00:00Z"})) }),
        )
        .service(service_tools("notes").url(notes))
        .mcp_server(
            McpServer::new("notes_mcp", Some(format!("{notes}/mcp")))
                .header("authorization", Some("Bearer mcp-secret".into())),
        )
        .build()
        .unwrap();
    agent.check().await.unwrap();
    let mut registry = ContractRegistry::for_service(ServiceContract::new("assistant", "1.0.0"));
    contract::register(&mut registry);
    let options = StreamOptions::new(
        Shutdown::new(),
        512 * 1024,
        Duration::from_secs(30),
        Duration::from_secs(5),
    )
    .unwrap();
    let router = mount_agent(
        ContractRouter::<()>::new(registry.clone()),
        agent,
        session_policy(),
        options,
    )
    .unwrap()
    .finish()
    .unwrap();
    let router = with_openapi(router, &registry, "/openapi.json", "v1").unwrap();
    serve(
        mount_http(
            router,
            &registry,
            McpServerOptions::new("assistant", "1.0.0"),
        )
        .unwrap(),
    )
    .await
}

#[tokio::test]
async fn mcp_client_lists_and_calls_the_framework_mcp_server() {
    let notes = notes_service().await;
    let refused = McpClient::connect(McpClientOptions::new(format!("{notes}/mcp"))).await;
    assert!(refused.is_err());
    let client = McpClient::connect(
        McpClientOptions::new(format!("{notes}/mcp")).header("authorization", "Bearer mcp-secret"),
    )
    .await
    .unwrap();
    let tools = client.list_tools().await.unwrap();
    let names: Vec<_> = tools.iter().map(|tool| tool.name.as_str()).collect();
    assert_eq!(names, vec!["listNotes", "status"]);
    let status = client.call_tool("status", json!({})).await.unwrap();
    assert!(!status.is_error);
    assert_eq!(status.structured_content, Some(json!({"ok": true})));
    client.close().await;
}

#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn agent_routes_list_tools_run_tasks_and_stream_events() {
    let notes = notes_service().await;
    let model = Arc::new(FakeModel::default());
    let base = agent_service(&notes, model.clone()).await;
    let http = reqwest::Client::new();

    let unauthenticated = http
        .get(format!("{base}/api/v1/agent/tools"))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), 401);

    let listing: Value = http
        .get(format!("{base}/api/v1/agent/tools"))
        .bearer_auth("user-token")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(listing["agent"], "assistant");
    assert_eq!(listing["model"], "luna");
    let tools = listing["tools"].as_array().unwrap();
    let find = |name: &str| {
        tools
            .iter()
            .find(|tool| tool["name"] == name)
            .cloned()
            .unwrap()
    };
    assert_eq!(
        find("current_time"),
        json!({"name": "current_time", "title": "Current time", "description": "The current UTC time.",
               "inputSchema": {"type": "object"}, "annotations": {"readOnlyHint": true},
               "source": {"kind": "local", "name": "assistant"}, "requiresApproval": false,
               "available": true, "unavailableReason": null})
    );
    let list_notes = find("notes__list_notes");
    assert_eq!(
        list_notes["source"],
        json!({"kind": "service", "name": "notes"})
    );
    assert_eq!(list_notes["requiresApproval"], false);
    let status = find("notes_mcp__status");
    assert_eq!(
        status["source"],
        json!({"kind": "mcp", "name": "notes_mcp"})
    );
    assert_eq!(status["annotations"], json!({"openWorldHint": true}));
    assert_eq!(status["requiresApproval"], true);
    assert!(tools.iter().all(|tool| tool["available"] == true));
    assert!(
        tools.iter().all(|tool| tool["name"] != "notes__sync_notes"),
        "internal operations are not agent tools"
    );
    let described: Value = http
        .get(format!("{notes}/api/v1/openapi"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let described = described.get("latest").unwrap_or(&described);
    assert_eq!(
        described["paths"]["/internal/notes/sync"]["post"]["x-forklaunch-access"],
        "internal"
    );

    // A service tool runs with the caller's own Authorization; an MCP tool waits for approval.
    model.responses.lock().unwrap().extend([
        tool_call("c1", "notes__list_notes"),
        tool_call("c2", "notes_mcp__status"),
        answer("You have one note and the service is up."),
    ]);
    let paused: Value = http
        .post(format!("{base}/api/v1/agent/tasks"))
        .bearer_auth("user-token")
        .json(&json!({"task": "What notes do I have?"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(paused["answer"], Value::Null);
    assert_eq!(paused["model"], "luna");
    assert_eq!(
        paused["toolCalls"],
        json!([{"id": "c1", "name": "notes__list_notes", "ok": true}])
    );
    assert_eq!(paused["approval"]["type"], "approval_required");
    assert_eq!(paused["approval"]["name"], "notes_mcp__status");
    let continuation = paused["approval"]["continuation"]
        .as_str()
        .unwrap()
        .to_owned();
    let notes_result = model.requests.lock().unwrap()[1]["messages"][3]["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        serde_json::from_str::<Value>(&notes_result).unwrap(),
        json!([{"id": "n1", "title": "Groceries"}])
    );

    let done: Value = http
        .post(format!("{base}/api/v1/agent/tasks"))
        .bearer_auth("user-token")
        .json(&json!({"continuation": continuation, "decision": "approve"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        done,
        json!({"answer": "You have one note and the service is up.", "model": "luna",
               "toolCalls": [{"id": "c2", "name": "notes_mcp__status", "ok": true}]})
    );
    let mcp_result = model.requests.lock().unwrap()[2]["messages"][5]["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        serde_json::from_str::<Value>(&mcp_result).unwrap(),
        json!({"ok": true})
    );

    let invalid = http
        .post(format!("{base}/api/v1/agent/tasks"))
        .bearer_auth("user-token")
        .json(&json!({"task": "x".repeat(8001)}))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid.status(), 400);
    for (fail, status) in [(429, 429), (500, 502)] {
        model
            .responses
            .lock()
            .unwrap()
            .push_back(json!({"fail": fail}));
        let refused = http
            .post(format!("{base}/api/v1/agent/tasks"))
            .bearer_auth("user-token")
            .json(&json!({"task": "hi"}))
            .send()
            .await
            .unwrap();
        assert_eq!(refused.status(), status);
    }
    let expired = http
        .post(format!("{base}/api/v1/agent/tasks"))
        .bearer_auth("user-token")
        .json(&json!({"continuation": "bogus", "decision": "approve"}))
        .send()
        .await
        .unwrap();
    assert_eq!(expired.status(), 400);

    model
        .responses
        .lock()
        .unwrap()
        .extend([tool_call("t1", "current_time"), answer("It is midnight.")]);
    let response = http
        .post(format!("{base}/api/v1/agent/tasks/stream"))
        .bearer_auth("user-token")
        .json(&json!({"task": "What time is it?"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(
        response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/event-stream")
    );
    let body = response.text().await.unwrap();
    let events: Vec<Value> = body
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(|data| serde_json::from_str(data.trim_start()).unwrap())
        .collect();
    assert_eq!(
        events,
        vec![
            json!({"type": "status", "message": "Agent is working"}),
            json!({"type": "tool_call", "id": "t1", "name": "current_time", "arguments": {}}),
            json!({"type": "tool_result", "id": "t1", "ok": true, "summary": "Completed."}),
            json!({"type": "delta", "text": "It is midnight."}),
            json!({"type": "done", "model": "luna"}),
        ]
    );

    // The routes are published contracts; the finite ones are MCP tools too.
    let document: Value = http
        .get(format!("{base}/api/v1/openapi"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    for path in [
        "/api/v1/agent/tools",
        "/api/v1/agent/tasks",
        "/api/v1/agent/tasks/stream",
    ] {
        assert!(document["latest"]["paths"].get(path).is_some(), "{path}");
    }
    let paths = &document["latest"]["paths"];
    assert_eq!(
        paths["/api/v1/agent/tools"]["get"]["operationId"],
        "ListAgentTools"
    );
    assert_eq!(
        paths["/api/v1/agent/tasks"]["post"]["operationId"],
        "RunAgentTask"
    );
    assert_eq!(
        paths["/api/v1/agent/tasks/stream"]["post"]["operationId"],
        "StreamAgentTask"
    );
    for status in ["429", "502", "503"] {
        assert!(
            paths["/api/v1/agent/tasks"]["post"]["responses"]
                .get(status)
                .is_some()
        );
    }
    let client = McpClient::connect(McpClientOptions::new(format!("{base}/mcp")))
        .await
        .unwrap();
    let names: Vec<_> = client
        .list_tools()
        .await
        .unwrap()
        .into_iter()
        .map(|tool| tool.name)
        .collect();
    assert_eq!(names, vec!["ListAgentTools", "RunAgentTask"]);
}

#[tokio::test]
async fn jwt_policy_rejects_missing_and_unverifiable_tokens() {
    use forklaunch_axum::agent::jwt_policy;
    use forklaunch_core::auth::jwt::{Algorithm, RemoteJwks, RemoteJwksConfig, Validation};
    let model = Arc::new(FakeModel::default());
    let agent = Agent::builder("assistant")
        .model("luna")
        .model_backend(model)
        .build()
        .unwrap();
    let mut registry = ContractRegistry::for_service(ServiceContract::new("assistant", "1.0.0"));
    contract::register(&mut registry);
    let jwks = RemoteJwks::new(RemoteJwksConfig::new("http://127.0.0.1:9/jwks")).unwrap();
    let options = StreamOptions::new(
        Shutdown::new(),
        512 * 1024,
        Duration::from_secs(30),
        Duration::from_secs(5),
    )
    .unwrap();
    // The agent may also come from application state.
    let router = mount_agent(
        ContractRouter::<Agent>::new(registry),
        |state: &Agent| state.clone(),
        jwt_policy(jwks, Validation::new(Algorithm::RS256)),
        options,
    )
    .unwrap()
    .finish()
    .unwrap()
    .with_state(agent);
    let base = serve(router).await;
    let http = reqwest::Client::new();
    let missing = http
        .get(format!("{base}/api/v1/agent/tools"))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 401);
    let forged = http
        .get(format!("{base}/api/v1/agent/tools"))
        .bearer_auth("not.a.jwt")
        .send()
        .await
        .unwrap();
    assert_eq!(forged.status(), 401);
}

#[tokio::test]
async fn agent_without_a_model_answers_503_on_both_task_routes() {
    // No `.model(..)`; the test environment does not set AGENT_MODEL.
    let agent = Agent::builder("assistant")
        .continuation_secret("agent-secret")
        .model_backend(Arc::new(FakeModel::default()))
        .build()
        .unwrap();
    let mut registry = ContractRegistry::for_service(ServiceContract::new("assistant", "1.0.0"));
    contract::register(&mut registry);
    let options = StreamOptions::new(
        Shutdown::new(),
        512 * 1024,
        Duration::from_secs(30),
        Duration::from_secs(5),
    )
    .unwrap();
    let router = mount_agent(
        ContractRouter::<()>::new(registry),
        agent,
        session_policy(),
        options,
    )
    .unwrap()
    .finish()
    .unwrap();
    let base = serve(router).await;
    let http = reqwest::Client::new();
    for path in ["/api/v1/agent/tasks", "/api/v1/agent/tasks/stream"] {
        let response = http
            .post(format!("{base}{path}"))
            .bearer_auth("user-token")
            .json(&json!({"task": "hi"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 503, "{path}");
    }
}
