use super::*;
use crate::{RouterConfig, session::AuthenticatedSession};
use axum::Json;
use forklaunch_core::ServiceInfo;
use forklaunch_core::auth::{AuthError, Principal};
use forklaunch_core::contract::{AccessLevel, BodyContract, RouteContract};
use forklaunch_core::mcp::server::McpServerOptions;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[tokio::test]
async fn separate_listeners_keep_route_auth_and_drain_both_active_surfaces() {
    let entered = Arc::new(AtomicUsize::new(0));
    let (http, mcp) = routers(entered.clone());
    let http_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mcp_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let http_base = format!("http://{}", http_listener.local_addr().unwrap());
    let mcp_base = format!("http://{}", mcp_listener.local_addr().unwrap());
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let serving = tokio::spawn(serve_listeners(
        http_listener,
        http,
        Some((mcp_listener, mcp)),
        async {
            let _ = stopped.await;
        },
    ));
    let client = reqwest::Client::new();
    assert_eq!(
        get_status(&client, format!("{http_base}/health")).await,
        200
    );
    assert_eq!(get_status(&client, format!("{mcp_base}/health")).await, 404);
    assert_eq!(
        client
            .post(format!("{http_base}/mcp"))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let arguments = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"slow","arguments":{}}});
    let rejected: Value = client
        .post(format!("{mcp_base}/mcp"))
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-11-25")
        .json(&arguments)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(rejected["result"]["isError"], true);
    assert_eq!(entered.load(Ordering::SeqCst), 0);
    let http_work = tokio::spawn(
        client
            .get(format!("{http_base}/slow"))
            .bearer_auth("tenant-a")
            .send(),
    );
    let mcp_work = tokio::spawn(
        client
            .post(format!("{mcp_base}/mcp"))
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", "2025-11-25")
            .bearer_auth("tenant-a")
            .json(&arguments)
            .send(),
    );
    tokio::time::timeout(Duration::from_secs(2), async {
        while entered.load(Ordering::SeqCst) != 2 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    stop.send(()).unwrap();
    assert!(!serving.is_finished());
    assert_eq!(
        http_work
            .await
            .unwrap()
            .unwrap()
            .json::<Value>()
            .await
            .unwrap(),
        json!({"finished":true})
    );
    let result = mcp_work
        .await
        .unwrap()
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        result["result"]["structuredContent"],
        json!({"finished":true})
    );
    tokio::time::timeout(Duration::from_secs(2), serving)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    for base in [http_base, mcp_base] {
        assert_closed(&base).await;
    }
}

fn routers(observed: Arc<AtomicUsize>) -> (Router, Router) {
    let config = RouterConfig::default().with_authenticator(|request, _| async move {
        if request.headers.get("authorization").map(String::as_str) != Some("Bearer tenant-a") {
            return Err(AuthError::MissingCredentials);
        }
        Ok(AuthenticatedSession::new(
            Principal::new("actor-a"),
            json!({"sub":"actor-a"}),
        ))
    });
    let application = ForkLaunchRouter::new(ServiceInfo::new("two-listener-fixture", "1.0.0"))
        .get(
            RouteContract::get("Health", "Health", "/health"),
            || async { Json(json!({"ok":true})) },
        )
        .get(
            RouteContract::get("Slow", "Slow authenticated work", "/slow")
                .with_access(AccessLevel::Authenticated)
                .with_response(200, Some(BodyContract::json("Slow"))),
            move || {
                let entered = observed.clone();
                async move {
                    entered.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    Json(json!({"finished":true}))
                }
            },
        )
        .with_config(config);
    application
        .into_routers_with_mcp(McpServerOptions::new("two-listener-fixture", "1.0.0"))
        .unwrap()
}

async fn get_status(client: &reqwest::Client, url: String) -> u16 {
    client.get(url).send().await.unwrap().status().as_u16()
}

async fn assert_closed(base: &str) {
    assert!(
        reqwest::Client::new()
            .get(format!("{base}/health"))
            .timeout(Duration::from_secs(1))
            .send()
            .await
            .is_err()
    );
}

fn basic_routers(entered: Arc<AtomicUsize>) -> (axum::Router, axum::Router) {
    use crate::session::Session;
    use forklaunch_core::auth::typescript::basic::verify_basic;
    use forklaunch_core::contract::RbacConfig;
    let observed = entered;
    let config = RouterConfig::default().with_response_authenticator(|request, _| async move {
        let principal = verify_basic(
            request.headers.get("authorization").map(String::as_str),
            None,
            |user, password| user == "user" && password == "pass",
        )?;
        let claims = json!({"sub":principal.subject});
        Ok(AuthenticatedSession::new(principal, claims))
    });
    let application = ForkLaunchRouter::new(ServiceInfo::new("basic-fixture", "1.0.0"))
        .get(
            RouteContract::get("Profile", "Verified profile", "/profile")
                .with_access(AccessLevel::Authenticated)
                .with_response(200, Some(BodyContract::json("Profile"))),
            move |Session::<Value>(claims)| {
                let entered = observed.clone();
                async move {
                    entered.fetch_add(1, Ordering::SeqCst);
                    Json(claims)
                }
            },
        )
        .get(
            RouteContract::get("Admin", "Requires admin role", "/admin")
                .with_access(AccessLevel::Protected)
                .with_rbac(RbacConfig {
                    allowed_roles: vec!["admin".into()],
                    ..RbacConfig::default()
                }),
            || async { "unreachable" },
        )
        .with_config(config);
    application
        .into_routers_with_mcp(McpServerOptions::new("basic-fixture", "1.0.0"))
        .unwrap()
}

#[tokio::test]
async fn explicit_basic_verification_guards_live_http_and_mcp_sessions() {
    let entered = Arc::new(AtomicUsize::new(0));
    let (http, mcp) = basic_routers(entered.clone());
    let http_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mcp_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let http_base = format!("http://{}", http_listener.local_addr().unwrap());
    let mcp_base = format!("http://{}", mcp_listener.local_addr().unwrap());
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let serving = tokio::spawn(serve_listeners(
        http_listener,
        http,
        Some((mcp_listener, mcp)),
        async {
            let _ = stopped.await;
        },
    ));
    let client = reqwest::Client::new();
    for (header, status, message) in [
        (None, 401, "Authorization token required."),
        (
            Some("basic dXNlcjpwYXNz"),
            401,
            "Invalid Authorization token format.",
        ),
        (
            Some("Basic dXNlcjo="),
            401,
            "Invalid Authorization token format.",
        ),
        (
            Some("Basic dXNlcjp3cm9uZw=="),
            403,
            "Invalid Authorization login.",
        ),
        (Some("Basic dXNlcjpwYXNzOmV4dHJh"), 200, ""),
    ] {
        let mut http = client.get(format!("{http_base}/profile"));
        let mut mcp = client.post(format!("{mcp_base}/mcp"))
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", "2025-11-25")
            .json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"profile","arguments":{}}}));
        if let Some(header) = header {
            http = http.header("authorization", header);
            mcp = mcp.header("authorization", header);
        }
        let response = http.send().await.unwrap();
        assert_eq!(response.status().as_u16(), status);
        let body = response.text().await.unwrap();
        let result: Value = mcp.send().await.unwrap().json().await.unwrap();
        if status == 200 {
            assert_eq!(
                serde_json::from_str::<Value>(&body).unwrap(),
                json!({"sub":"user"})
            );
            assert_eq!(result["result"]["structuredContent"], json!({"sub":"user"}));
        } else {
            assert!(body.contains(message));
            assert_eq!(result["result"]["isError"], true);
            assert_eq!(entered.load(Ordering::SeqCst), 0);
        }
    }
    assert_eq!(entered.load(Ordering::SeqCst), 2);
    assert_eq!(
        client
            .get(format!("{http_base}/admin"))
            .basic_auth("user", Some("pass"))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), serving)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_closed(&http_base).await;
    assert_closed(&mcp_base).await;
}
