#![cfg(all(feature = "websocket", feature = "mcp"))]
use axum::{
    extract::{State, ws::WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use forklaunch_axum::{
    ForkLaunchRouter,
    websocket::{self, AxumTypedSocket},
};
use forklaunch_core::ServiceInfo;
use forklaunch_core::contract::RouteContract;
use forklaunch_core::mcp::server::McpServerOptions;
use forklaunch_ws::{
    codec::PayloadCodec,
    transport::{self, IntoClientRequest, SocketError, SocketEvent, SocketOptions, SocketSchemas},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{net::TcpListener, sync::watch, time::timeout};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Request {
    message: String,
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Reply {
    accepted: String,
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Stamp {
    ts: u64,
}
fn schemas() -> SocketSchemas<Request, Reply, Stamp, Stamp> {
    SocketSchemas::new(
        PayloadCodec::serde().with_validator(|input: &Request| {
            if input.message.is_empty() {
                Err("empty message".into())
            } else {
                Ok(())
            }
        }),
        PayloadCodec::serde(),
    )
    .with_ping(PayloadCodec::serde())
    .with_pong(PayloadCodec::serde())
}
#[derive(Clone)]
struct AppState {
    entered: Arc<AtomicUsize>,
    stop: watch::Receiver<bool>,
}
async fn upgrade(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        != Some("Bearer fixture-token")
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    websocket::upgrade(
        ws.protocols(["chat.v1"]).on_failed_upgrade(|error| {
            tracing::warn!(%error, "fixture upgrade failed");
        }),
        schemas(),
        SocketOptions {
            max_message_bytes: 256,
            ..SocketOptions::default()
        },
        move |socket| session(socket, state),
    )
    .unwrap()
}
async fn session(mut socket: AxumTypedSocket<Request, Reply, Stamp, Stamp>, mut state: AppState) {
    state.entered.fetch_add(1, Ordering::SeqCst);
    loop {
        tokio::select! {
            _ = state.stop.changed() => {
                socket.close(1001, Some(&json!("shutdown"))).await.unwrap();
                let _ = timeout(Duration::from_secs(1), socket.recv()).await;
                return;
            }
            event = socket.recv() => match event {
                Ok(Some(SocketEvent::Message(message))) => {
                    socket.send(&Reply { accepted: message.message }).await.unwrap();
                }
                Ok(Some(SocketEvent::Ping(_) | SocketEvent::Pong(_))) => {}
                Ok(Some(SocketEvent::Close { .. }) | None) | Err(_) => return,
            }
        }
    }
}
async fn http_mcp_assertions(base: &str) {
    let client = reqwest::Client::new();
    assert_eq!(
        client
            .get(format!("{base}/health"))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let response: Value = client.post(format!("{base}/mcp")).header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-11-25")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"health","arguments":{}}}))
        .send().await.unwrap().json().await.unwrap();
    assert_eq!(
        response["result"]["structuredContent"],
        json!({"ok":true}),
        "{response}"
    );
    assert_ne!(response["result"]["isError"], true, "{response}");
    assert!(response.get("error").is_none(), "{response}");
}
async fn normal_peer(url: &str) {
    let mut request = url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("authorization", "Bearer fixture-token".parse().unwrap());
    request
        .headers_mut()
        .insert("sec-websocket-protocol", "chat.v1".parse().unwrap());
    let (mut socket, response) =
        transport::connect_with_response(request, schemas().reversed(), SocketOptions::default())
            .await
            .unwrap();
    assert_eq!(response.headers()["sec-websocket-protocol"], "chat.v1");
    socket
        .send(&Request {
            message: "hello 水 🦀".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        socket.recv().await.unwrap(),
        Some(SocketEvent::Message(Reply {
            accepted: "hello 水 🦀".into()
        }))
    );
    socket.ping(Some(&Stamp { ts: 42 })).await.unwrap();
    assert_eq!(
        socket.recv().await.unwrap(),
        Some(SocketEvent::Pong(Stamp { ts: 42 }))
    );
    socket.close(1000, Some(&json!("done"))).await.unwrap();
    assert_eq!(
        socket.recv().await.unwrap(),
        Some(SocketEvent::Close {
            code: 1000,
            reason: json!("done")
        })
    );
}
async fn invalid_peers(url: &str) {
    for oversized in [false, true] {
        let mut request = url.into_client_request().unwrap();
        request
            .headers_mut()
            .insert("authorization", "Bearer fixture-token".parse().unwrap());
        let mut socket = transport::connect(
            request,
            SocketSchemas::new(
                PayloadCodec::<Value>::serde(),
                PayloadCodec::<String>::serde(),
            ),
            SocketOptions::default(),
        )
        .await
        .unwrap();
        socket
            .send(&if oversized {
                "x".repeat(1024)
            } else {
                "not a request".into()
            })
            .await
            .unwrap();
        if oversized {
            assert!(socket.recv().await.is_err());
        } else {
            assert!(matches!(
                socket.recv().await.unwrap(),
                Some(SocketEvent::Close { code: 1008, .. })
            ));
        }
        assert!(socket.recv().await.unwrap().is_none());
    }
}
#[tokio::test]
async fn typed_upgrades_share_http_and_mcp_listener_with_auth_protocols_and_shutdown() {
    let (stop, stopped) = watch::channel(false);
    let entered = Arc::new(AtomicUsize::new(0));
    let app = ForkLaunchRouter::new(ServiceInfo::new("realtime", "0.1.0"))
        .get(
            RouteContract::get("health", "Health", "/health").with_response(200, None),
            || async { axum::Json(json!({"ok":true})) },
        )
        .into_router_with_mcp(McpServerOptions::new("realtime", "0.1.0"))
        .unwrap()
        .merge(
            axum::Router::new()
                .route("/ws", get(upgrade))
                .with_state(AppState {
                    entered: entered.clone(),
                    stop: stopped.clone(),
                }),
        );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let serving = tokio::spawn(async move {
        let mut stopped = stopped;
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = stopped.changed().await;
            })
            .await
            .unwrap();
    });
    let url = format!("ws://{address}/ws");
    assert!(matches!(
        transport::connect(url.as_str(), schemas().reversed(), SocketOptions::default()).await,
        Err(SocketError::Transport(_))
    ));
    assert_eq!(entered.load(Ordering::SeqCst), 0);
    http_mcp_assertions(&format!("http://{address}")).await;
    normal_peer(&url).await;
    invalid_peers(&url).await;
    let mut request = url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("authorization", "Bearer fixture-token".parse().unwrap());
    let mut active = transport::connect(request, schemas().reversed(), SocketOptions::default())
        .await
        .unwrap();
    http_mcp_assertions(&format!("http://{address}")).await;
    stop.send(true).unwrap();
    assert_eq!(
        active.recv().await.unwrap(),
        Some(SocketEvent::Close {
            code: 1001,
            reason: json!("shutdown")
        })
    );
    timeout(Duration::from_secs(3), serving)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(entered.load(Ordering::SeqCst), 4);
}
