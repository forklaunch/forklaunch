#![cfg(feature = "mcp")]
use axum::{
    Json,
    extract::{Path, Query},
    http::HeaderMap,
};
use forklaunch_axum::session::AuthenticatedSession;
use forklaunch_axum::{ForkLaunchRouter, RouterConfig};
use forklaunch_core::ServiceInfo;
use forklaunch_core::auth::{AuthError, Principal};
use forklaunch_core::contract::{
    AccessLevel, BodyContract, FieldContract, RouteContract, RouteOptions,
};
use forklaunch_core::mcp::{McpAuthCallback, server::McpServerOptions};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc};

async fn rpc(client: &reqwest::Client, url: &str, method: &str, params: Value) -> Value {
    let response = client
        .post(url)
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-11-25")
        .bearer_auth("transport-secret")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
        .send()
        .await
        .unwrap();
    assert!(
        response.status().is_success(),
        "{}",
        response.text().await.unwrap()
    );
    response.json().await.unwrap()
}
struct Gate;
impl McpAuthCallback for Gate {
    fn authorize(&self, token: Option<&str>) -> bool {
        token == Some("transport-secret")
    }
}
#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn live_http_and_mcp_initialize_discover_invoke_auth_validate_and_shutdown() {
    let config = RouterConfig::default().with_authenticator(|request, _| async move {
        if request.headers.get("authorization").map(String::as_str) != Some("route-secret") {
            return Err(AuthError::MissingCredentials);
        }
        Ok(AuthenticatedSession::new(
            Principal::new("verified"),
            json!({"sub":"verified"}),
        ))
    });
    let internal = axum::Router::new().route(
        "/session",
        axum::routing::get(|headers: HeaderMap| async move {
            if headers.get("cookie").and_then(|value| value.to_str().ok())
                == Some("fixture=session")
            {
                (axum::http::StatusCode::OK, Json(json!({"internal":true})))
            } else {
                (
                    axum::http::StatusCode::UNAUTHORIZED,
                    Json(json!({"internal":false})),
                )
            }
        }),
    );
    let service=ForkLaunchRouter::new(ServiceInfo::new("startup-test","1.2.3"))
        .get(RouteContract::get("Health","Health","/health").with_response(200,Some(BodyContract::json("Health"))),||async{Json(json!({"ok":true}))})
        .get(RouteContract::get("Read Item","Read item","/items/:id").with_param(FieldContract::required("id","String")).with_query(FieldContract::required("q","String")).with_access(AccessLevel::Authenticated).with_response(200,Some(BodyContract::json("Item"))),|Path(id):Path<String>,Query(query):Query<HashMap<String,String>>,headers:HeaderMap|async move {Json(json!({"id":id,"q":query["q"],"authorization":headers["authorization"].to_str().unwrap()}))})
        .get(RouteContract::get("Read Query","Read composite query","/query").with_query(FieldContract::optional("ids","Vec<String>")).with_response(200,Some(BodyContract::json("Query"))),|Query(query):Query<HashMap<String,String>>|async move {Json(query)})
        .post(RouteContract::post("Echo","Echo","/echo").with_request_body(BodyContract::json("Echo")).with_response(200,Some(BodyContract::json("Echo"))),|Json(body):Json<Value>|async move{Json(body)})
        .get(RouteContract::get("Hidden","Hidden","/hidden").with_options(RouteOptions{mcp:false,..RouteOptions::default()}),||async{"hidden"})
        .nest_internal("/api/auth", internal)
        .with_config(config);
    let decorated = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let layer_count = Arc::clone(&decorated);
    let service = service
        .map_route("/echo", move |route| {
            let layer_count = Arc::clone(&layer_count);
            route.layer(axum::middleware::from_fn(
                move |request: axum::extract::Request, next: axum::middleware::Next| {
                    let layer_count = Arc::clone(&layer_count);
                    async move {
                        layer_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        let mut response = next.run(request).await;
                        response.headers_mut().insert(
                            "x-route-decorated",
                            axum::http::HeaderValue::from_static("true"),
                        );
                        response
                    }
                },
            ))
        })
        .unwrap();
    let mut options = McpServerOptions::new("startup-test", "1.2.3");
    options.authorize = Arc::new(Gate);
    let app = service.into_router_with_mcp(options).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (shutdown, stopped) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
    });
    let client = reqwest::Client::new();
    let url = format!("{base}/mcp");
    assert_eq!(
        client
            .get(format!("{base}/api/auth/session"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let internal = client
        .get(format!("{base}/api/auth/session"))
        .header("cookie", "fixture=session")
        .send()
        .await
        .unwrap();
    assert_eq!(internal.status(), 200);
    assert_eq!(
        internal.json::<Value>().await.unwrap(),
        json!({"internal":true})
    );
    let document = client
        .get(format!("{base}{}", RouterConfig::default().openapi_path))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert!(document["paths"].get("/api/auth/session").is_none());
    let direct_echo = client
        .post(format!("{base}/echo"))
        .json(&json!({"direct":true}))
        .send()
        .await
        .unwrap();
    assert_eq!(direct_echo.headers()["x-route-decorated"], "true");
    assert_eq!(decorated.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        client
            .get(format!("{base}/health"))
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap(),
        json!({"ok":true})
    );
    assert_eq!(
        client
            .post(&url)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let init=rpc(&client,&url,"initialize",json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"fixture","version":"1.0.0"}})).await;
    assert_eq!(init["result"]["serverInfo"]["name"], "startup-test");
    let list = rpc(&client, &url, "tools/list", json!({})).await;
    let names: Vec<_> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["echo", "health", "readItem", "readQuery"]);
    let health = rpc(
        &client,
        &url,
        "tools/call",
        json!({"name":"health","arguments":{}}),
    )
    .await;
    assert_eq!(health["result"]["structuredContent"], json!({"ok":true}));
    let args = json!({"params":{"id":"space + 💬"},"query":{"q":"a+b & c"},"headers":{"authorization":"route-secret"}});
    let result = rpc(
        &client,
        &url,
        "tools/call",
        json!({"name":"readItem","arguments":args}),
    )
    .await;
    assert_eq!(
        result["result"]["structuredContent"],
        json!({"id":"space + 💬","q":"a+b & c","authorization":"route-secret"})
    );
    for ids in [json!(["a,b", "水"]), json!([])] {
        let result = rpc(
            &client,
            &url,
            "tools/call",
            json!({"name":"readQuery","arguments":{"query":{"ids":ids}}}),
        )
        .await;
        assert_eq!(
            result["result"]["structuredContent"]["ids"],
            ids.to_string()
        );
        let direct = client
            .get(format!("{base}/query"))
            .query(&[("ids", ids.to_string())])
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(result["result"]["structuredContent"], direct);
    }
    for args in [
        json!({"params":{"id":"x"},"query":{"q":"a"}}),
        json!({"params":{"id":"x"},"headers":{"authorization":"route-secret"}}),
    ] {
        let result = rpc(
            &client,
            &url,
            "tools/call",
            json!({"name":"readItem","arguments":args}),
        )
        .await;
        assert!(
            result.get("error").is_some() || result["result"]["isError"] == true,
            "{result}"
        );
    }
    let echo = rpc(
        &client,
        &url,
        "tools/call",
        json!({"name":"echo","arguments":{"body":{"text":"JSON 💬","list":[true,null,42]}}}),
    )
    .await;
    assert_eq!(
        echo["result"]["structuredContent"],
        json!({"text":"JSON 💬","list":[true,null,42]})
    );
    assert_eq!(decorated.load(std::sync::atomic::Ordering::SeqCst), 2);
    let hidden = rpc(
        &client,
        &url,
        "tools/call",
        json!({"name":"hidden","arguments":{}}),
    )
    .await;
    assert!(hidden.get("error").is_some());
    for (header, value) in [
        ("host", "attacker.example"),
        ("origin", "https://attacker.example"),
    ] {
        let response = client
            .post(&url)
            .bearer_auth("transport-secret")
            .header(header, value)
            .header("accept", "application/json, text/event-stream")
            .json(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 403);
    }
    shutdown.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
}
#[test]
fn conflicting_mcp_routes_fail_at_startup() {
    for path in [
        "/docs",
        "/openapi.json",
        "/",
        "/mcp/{id}",
        "/bad path",
        "/health",
    ] {
        let service = ForkLaunchRouter::new(ServiceInfo::new("test", "1.0.0")).get(
            RouteContract::get("Health", "Health", "/health"),
            || async { "ok" },
        );
        let mut options = McpServerOptions::new("test", "1.0.0");
        options.path = path.into();
        assert!(service.into_router_with_mcp(options).is_err(), "{path}");
    }
}
