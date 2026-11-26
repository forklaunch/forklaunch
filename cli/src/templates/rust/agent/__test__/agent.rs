//! Agent routes: JWT auth, the tool listing and approval for write tools.
//! The model is scripted and sessions come from a test verifier, so these
//! tests need no network, model connection or identity provider.
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use forklaunch_axum::agent::jwt_policy;
use forklaunch_axum::native::SessionPolicy;
use forklaunch_axum::session::{AuthenticatedSession, AuthenticationFuture};
use forklaunch_core::agent::{AgentBuilder, BoxFuture, ModelBackend, ModelChunks, ModelError, Tool};
use forklaunch_core::auth::jwt::{Algorithm, RemoteJwks, RemoteJwksConfig, Validation};
use forklaunch_core::auth::{AuthError, Principal};
use forklaunch_core::http::FrameworkRequest;
use forklaunch_core::contract::RouteContract;
use {{crate_module}}::agent::{AgentRuntime, builder};
use {{crate_module}}::config::ServiceConfig;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::env;
use std::sync::{Arc, Mutex, Once};
use tower::ServiceExt;

/// Replies with the scripted responses in order.
struct ScriptedModel(Mutex<VecDeque<Value>>);
impl ModelBackend for ScriptedModel {
    fn complete(&self, _: Value) -> BoxFuture<'_, Result<Value, ModelError>> {
        let next = self.0.lock().unwrap().pop_front();
        Box::pin(async move { next.ok_or(ModelError::status(500)) })
    }
    fn stream(&self, _: Value) -> BoxFuture<'_, Result<Box<dyn ModelChunks>, ModelError>> {
        Box::pin(async { Err(ModelError::status(501)) })
    }
}

fn scripted(agent: AgentBuilder, responses: Vec<Value>) -> AgentBuilder {
    agent
        .model("test-model")
        .streaming(false)
        .continuation_secret("test-continuation-secret")
        .model_backend(Arc::new(ScriptedModel(Mutex::new(responses.into()))))
}

/// Accepts `Bearer test-token` as user `user-1`.
fn test_sessions() -> SessionPolicy {
    SessionPolicy::new(
        Arc::new(|request: FrameworkRequest, _: RouteContract| {
            Box::pin(async move {
                match request.headers.get("authorization").map(String::as_str) {
                    Some("Bearer test-token") => {
                        Ok(AuthenticatedSession::new(Principal::new("user-1"), json!({"sub": "user-1"})))
                    }
                    Some(_) => Err(AuthError::InvalidCredentials),
                    None => Err(AuthError::MissingCredentials),
                }
            }) as AuthenticationFuture
        }),
        Arc::new(|_, _| Ok(())),
    )
}

fn router(runtime: AgentRuntime) -> Router {
    // `register` requires a JWKS URL at startup; every test replaces the auth
    // policy below, so this placeholder is never contacted.
    static JWKS: Once = Once::new();
    JWKS.call_once(|| {
        if env::var_os("JWKS_PUBLIC_KEY_URL").is_none() {
            // SAFETY: set once, before any test reads the setting.
            unsafe { env::set_var("JWKS_PUBLIC_KEY_URL", "http://127.0.0.1:9/jwks") };
        }
    });
    let mut state = {{crate_module}}::register(ServiceConfig::default()).unwrap();
    state.agent = runtime;
    {{crate_module}}::server::router(state).unwrap()
}

async fn send(router: &Router, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", "Bearer test-token")
        .header("content-type", "application/json");
    let body = body.map_or_else(Body::empty, |body| Body::from(body.to_string()));
    let response = router.clone().oneshot(request.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn agent_routes_require_a_verified_token() {
    let jwks = RemoteJwks::new(RemoteJwksConfig::new("http://127.0.0.1:9/jwks")).unwrap();
    let auth = jwt_policy(jwks, Validation::new(Algorithm::RS256));
    let router = router(AgentRuntime::new(scripted(builder(), vec![]).build().unwrap(), auth));
    let response = router
        .oneshot(Request::builder().uri("/api/v1/agent/tools").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tools_list_the_read_only_example() {
    let agent = scripted(builder(), vec![]).build().unwrap();
    agent.check().await.unwrap();
    let router = router(AgentRuntime::new(agent, test_sessions()));
    let (status, listing) = send(&router, "GET", "/api/v1/agent/tools", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listing["model"], "test-model");
    let tool = listing["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "current_time")
        .cloned()
        .unwrap();
    assert_eq!(tool["requiresApproval"], false);
    assert_eq!(tool["available"], true);
    assert_eq!(tool["source"]["kind"], "local");
}

#[tokio::test]
async fn write_tools_wait_for_approval() {
    let calls = Arc::new(Mutex::new(0));
    let counted = calls.clone();
    let write = Tool::new(
        "create_draft",
        "Create draft",
        "Creates a draft note.",
        json!({"type": "object", "properties": {"title": {"type": "string"}}}),
    )
    .handler(move |_, _| {
        let counted = counted.clone();
        async move {
            *counted.lock().unwrap() += 1;
            Ok(json!({"id": "draft-1"}))
        }
    });
    let model = vec![
        json!({"choices": [{"index": 0, "finish_reason": "tool_calls", "message": {"role": "assistant", "content": null,
            "tool_calls": [{"id": "call-1", "type": "function", "function": {"name": "create_draft", "arguments": "{\"title\":\"Groceries\"}"}}]}}]}),
        json!({"choices": [{"index": 0, "finish_reason": "stop", "message": {"role": "assistant", "content": "Created the draft."}}]}),
    ];
    let agent = scripted(builder().tool(write), model).build().unwrap();
    let router = router(AgentRuntime::new(agent, test_sessions()));
    let (status, paused) = send(&router, "POST", "/api/v1/agent/tasks", Some(json!({"task": "Draft a grocery note"}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(paused["answer"], Value::Null);
    assert_eq!(paused["approval"]["type"], "approval_required");
    assert_eq!(paused["approval"]["name"], "create_draft");
    assert_eq!(*calls.lock().unwrap(), 0);
    let continuation = paused["approval"]["continuation"].clone();
    let (status, done) = send(
        &router,
        "POST",
        "/api/v1/agent/tasks",
        Some(json!({"continuation": continuation, "decision": "approve"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(done["answer"], "Created the draft.");
    assert_eq!(done["toolCalls"][0]["name"], "create_draft");
    assert_eq!(*calls.lock().unwrap(), 1);
}
