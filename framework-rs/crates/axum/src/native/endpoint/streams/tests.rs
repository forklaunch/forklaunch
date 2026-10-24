use super::*;
use axum::{
    body::{Body, to_bytes},
    http::request::Parts,
};
use forklaunch_core::contract::{AccessLevel, ContractRegistry, endpoint::NoBody};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tower::ServiceExt;
#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema, Debug, PartialEq)]
pub struct Message {
    #[schemars(length(min = 3))]
    text: String,
}
#[derive(Clone)]
struct Policy {
    allowed: Arc<AtomicBool>,
    checks: Arc<AtomicUsize>,
}
impl Policy {
    fn new() -> Self {
        Self {
            allowed: Arc::new(AtomicBool::new(true)),
            checks: Arc::new(AtomicUsize::new(0)),
        }
    }
}
#[allow(clippy::unused_async_trait_impl)]
impl EndpointPolicy<()> for Policy {
    type Context = usize;
    async fn authorize(
        &self,
        _: &mut Parts,
        (): &(),
        _: &RouteContract,
    ) -> Result<usize, EndpointError> {
        let generation = self.checks.fetch_add(1, Ordering::SeqCst);
        if self.allowed.load(Ordering::SeqCst) {
            Ok(generation)
        } else {
            Err(EndpointError::Forbidden)
        }
    }
}
fn options(shutdown: Shutdown) -> StreamOptions {
    StreamOptions::new(
        shutdown,
        1024,
        Duration::from_millis(10),
        Duration::from_millis(40),
    )
    .unwrap()
}
fn sse() -> Endpoint<NoBody, SseBody<Message>> {
    Endpoint::new("Events", "Events", HttpMethod::Get, "/events", 200)
        .unwrap()
        .with_access(AccessLevel::Authenticated)
}
#[tokio::test]
async fn post_sse_validates_typed_input_and_preserves_declared_status() {
    use forklaunch_core::contract::endpoint::JsonBody;
    let endpoint = Endpoint::<JsonBody<Message>, SseBody<Message>>::new(
        "Post events",
        "PostEvents",
        HttpMethod::Post,
        "/events",
        201,
    )
    .unwrap()
    .with_access(AccessLevel::Authenticated);
    let mut registry = ContractRegistry::new();
    registry.register(endpoint.contract());
    let calls = Arc::new(AtomicUsize::new(0));
    let handler_calls = calls.clone();
    let app = ContractRouter::<()>::new(registry)
        .bind_sse_authorized(
            endpoint,
            Policy::new(),
            options(Shutdown::new()),
            move |_, _, input: Message| {
                handler_calls.fetch_add(1, Ordering::SeqCst);
                async move { Ok(futures_util::stream::iter([Ok(ServerEvent::new(input))])) }
            },
        )
        .unwrap()
        .finish()
        .unwrap();
    for (body, expected) in [(r#"{"text":"hello"}"#, 201), (r#"{"text":"x"}"#, 400)] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/events")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), expected);
        if expected == 201 {
            assert_eq!(response.headers()["content-type"], "text/event-stream");
            let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
            assert!(
                String::from_utf8(bytes.to_vec())
                    .unwrap()
                    .contains(r#"data: {"text":"hello"}"#)
            );
        }
    }
    let response = app
        .oneshot(
            Request::builder()
                .uri("/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn sse_binds_schema_excludes_mcp_and_checks_every_frame() {
    let endpoint = sse();
    assert!(!endpoint.contract().options.mcp);
    let mut registry = ContractRegistry::new();
    registry.register(endpoint.contract());
    let policy = Policy::new();
    let app = ContractRouter::<()>::new(registry)
        .bind_sse_authorized(
            endpoint,
            policy.clone(),
            options(Shutdown::new()),
            |_, _, ()| async {
                Ok(futures_util::stream::iter([
                    Ok(ServerEvent::new(Message {
                        text: "hello".into(),
                    })
                    .with_id("1")
                    .with_event("note")),
                    Ok(ServerEvent::new(Message { text: "x".into() })),
                    Ok(ServerEvent::new(Message {
                        text: "must not escape".into(),
                    })),
                ]))
            },
        )
        .unwrap()
        .finish()
        .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("data: {\"text\":\"hello\"}"));
    assert!(text.contains("id: 1"));
    assert!(!text.contains("must not escape"));
    assert!(!text.contains("\"x\""));
    assert!(policy.checks.load(Ordering::SeqCst) >= 3);
}
#[tokio::test]
async fn sse_typed_headers_are_validated_before_any_frame_escapes() {
    #[derive(serde::Serialize, schemars::JsonSchema)]
    struct Headers {
        #[serde(rename = "x-sequence")]
        #[schemars(range(min = 1))]
        sequence: u32,
    }
    for sequence in [7, 0] {
        let endpoint = sse().with_typed_response_headers::<Headers>().unwrap();
        let mut registry = ContractRegistry::new();
        registry.register(endpoint.contract());
        let app = ContractRouter::<()>::new(registry)
            .bind_sse_authorized_with_headers(
                endpoint,
                Policy::new(),
                options(Shutdown::new()),
                move |_, _, ()| async move {
                    Ok(ResponseWithHeaders {
                        headers: Headers { sequence },
                        body: futures_util::stream::iter([Ok(ServerEvent::new(Message {
                            text: "hello".into(),
                        }))]),
                    })
                },
            )
            .unwrap()
            .finish()
            .unwrap();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status().as_u16(),
            if sequence == 7 { 200 } else { 500 }
        );
        if sequence == 7 {
            assert_eq!(response.headers()["x-sequence"], "7");
        }
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        assert_eq!(
            String::from_utf8(bytes.to_vec()).unwrap().contains("hello"),
            sequence == 7
        );
    }
}
#[tokio::test]
async fn idle_sse_revocation_and_shutdown_close_without_waiting_for_events() {
    for revoke in [true, false] {
        let endpoint = sse();
        let mut registry = ContractRegistry::new();
        registry.register(endpoint.contract());
        let shutdown = Shutdown::new();
        let policy = Policy::new();
        let app = ContractRouter::<()>::new(registry)
            .bind_sse_authorized(
                endpoint,
                policy.clone(),
                options(shutdown.clone()),
                |_, _, ()| async {
                    Ok(futures_util::stream::pending::<
                        Result<ServerEvent<Message>, EndpointError>,
                    >())
                },
            )
            .unwrap()
            .finish()
            .unwrap();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if revoke {
            policy.allowed.store(false, Ordering::SeqCst);
        } else {
            shutdown.trigger();
        }
        let body =
            tokio::time::timeout(Duration::from_secs(1), to_bytes(response.into_body(), 4096))
                .await
                .unwrap()
                .unwrap();
        assert!(body.is_empty());
    }
}
#[tokio::test]
async fn denied_stream_never_enters_application_and_keeps_policy_status() {
    let endpoint = sse();
    let mut registry = ContractRegistry::new();
    registry.register(endpoint.contract());
    let policy = Policy::new();
    policy.allowed.store(false, Ordering::SeqCst);
    let app = ContractRouter::<()>::new(registry)
        .bind_sse_authorized(
            endpoint,
            policy,
            options(Shutdown::new()),
            |_, _, ()| async {
                panic!("unauthorized application called");
                #[allow(unreachable_code)]
                Ok(futures_util::stream::empty::<
                    Result<ServerEvent<Message>, EndpointError>,
                >())
            },
        )
        .unwrap()
        .finish()
        .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
}
fn socket() -> forklaunch_ws::TypedSocket<NoBody, Message, Message> {
    forklaunch_ws::TypedSocket::new(
        RouteContract::get("Presence", "Presence", "/presence")
            .with_access(AccessLevel::Authenticated),
        "presence",
        "presence",
    )
    .unwrap()
}
#[test]
fn socket_registry_requires_exact_bound_handshake_and_schemas() {
    let endpoint = socket();
    let mut registry = ContractRegistry::new();
    registry
        .register_socket("presence", &endpoint.contract())
        .unwrap();
    assert!(
        ContractRouter::<()>::new(registry.clone())
            .finish()
            .is_err()
    );
    let mut changed = endpoint.contract();
    changed.server_messages.clear();
    let mut drifted = ContractRegistry::new();
    drifted.register_socket("presence", &changed).unwrap();
    assert!(
        ContractRouter::<()>::new(drifted)
            .bind_socket_authorized(
                "presence",
                endpoint,
                Policy::new(),
                options(Shutdown::new()),
                |_, _, (), _| async {}
            )
            .is_err()
    );
    let bound = ContractRouter::<()>::new(registry)
        .bind_socket_authorized(
            "presence",
            socket(),
            Policy::new(),
            options(Shutdown::new()),
            |_, _, (), _| async {},
        )
        .unwrap();
    assert!(
        bound
            .bind_socket_authorized(
                "presence",
                socket(),
                Policy::new(),
                options(Shutdown::new()),
                |_, _, (), _| async {}
            )
            .is_err()
    );
}
#[tokio::test]
async fn real_websocket_typed_echo_revocation_invalid_input_and_shutdown() {
    use forklaunch_ws::{
        codec::PayloadCodec,
        transport::{SocketEvent, SocketOptions, SocketSchemas, connect},
    };
    for mode in ["revoke", "invalid", "shutdown"] {
        let endpoint = socket();
        let mut registry = ContractRegistry::new();
        registry
            .register_socket("presence", &endpoint.contract())
            .unwrap();
        let policy = Policy::new();
        let shutdown = Shutdown::new();
        let delivered = Arc::new(AtomicUsize::new(0));
        let count = delivered.clone();
        let app = ContractRouter::<()>::new(registry)
            .bind_socket_authorized(
                "presence",
                endpoint,
                policy.clone(),
                options(shutdown.clone()),
                move |_, initial, (), mut session| {
                    let count = count.clone();
                    async move {
                        while let Some((current, message)) = session.recv().await {
                            assert!(current > initial);
                            count.fetch_add(1, Ordering::SeqCst);
                            if session.send(message).await.is_err() {
                                break;
                            }
                        }
                    }
                },
            )
            .unwrap()
            .finish()
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let schemas = SocketSchemas::<Message, serde_json::Value>::new(
            PayloadCodec::serde().with_json_encoding(),
            PayloadCodec::serde().with_json_encoding(),
        );
        let mut client = connect(
            format!("ws://{address}/presence"),
            schemas,
            SocketOptions::default(),
        )
        .await
        .unwrap();
        client
            .send(&serde_json::json!({"text":"hello"}))
            .await
            .unwrap();
        let received = client.recv().await.unwrap();
        assert!(
            matches!(&received,Some(SocketEvent::Message(Message{text})) if text=="hello"),
            "unexpected response {received:?}"
        );
        assert_eq!(delivered.load(Ordering::SeqCst), 1);
        match mode {
            "revoke" => policy.allowed.store(false, Ordering::SeqCst),
            "shutdown" => shutdown.trigger(),
            _ => client.send(&serde_json::json!({"text":"x"})).await.unwrap(),
        }
        let closed = tokio::time::timeout(Duration::from_secs(1), client.recv())
            .await
            .expect("socket must close promptly");
        assert!(!matches!(closed, Ok(Some(SocketEvent::Message(_)))));
        assert_eq!(delivered.load(Ordering::SeqCst), 1);
        server.abort();
    }
}

#[tokio::test]
async fn sse_stalled_request_body_cannot_hold_authorized_connection_forever() {
    let endpoint = sse();
    let mut registry = ContractRegistry::new();
    registry.register(endpoint.contract());
    let app = ContractRouter::<()>::new(registry)
        .bind_sse_authorized(
            endpoint,
            Policy::new(),
            options(Shutdown::new()),
            |_, _, ()| async {
                panic!("stalled body entered handler");
                #[allow(unreachable_code)]
                Ok(futures_util::stream::empty::<
                    Result<ServerEvent<Message>, EndpointError>,
                >())
            },
        )
        .unwrap()
        .finish()
        .unwrap();
    let body = Body::from_stream(futures_util::stream::pending::<
        Result<bytes::Bytes, std::io::Error>,
    >());
    let response = tokio::time::timeout(
        Duration::from_secs(1),
        app.oneshot(Request::builder().uri("/events").body(body).unwrap()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(response.status(), 413);
}

#[tokio::test]
async fn sse_slow_consumer_stops_producer_at_bounded_queue() {
    let endpoint = sse();
    let mut registry = ContractRegistry::new();
    registry.register(endpoint.contract());
    let produced = Arc::new(AtomicUsize::new(0));
    let counter = produced.clone();
    let app = ContractRouter::<()>::new(registry)
        .bind_sse_authorized(
            endpoint,
            Policy::new(),
            options(Shutdown::new()),
            move |_, _, ()| {
                let produced = counter.clone();
                async move {
                    Ok(futures_util::stream::repeat_with(move || {
                        produced.fetch_add(1, Ordering::SeqCst);
                        Ok(ServerEvent::new(Message {
                            text: "hello".into(),
                        }))
                    }))
                }
            },
        )
        .unwrap()
        .finish()
        .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(produced.load(Ordering::SeqCst), 2);
    // Only the one already queued event remains after the producer closes.
    let body = tokio::time::timeout(Duration::from_secs(1), to_bytes(response.into_body(), 4096))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        String::from_utf8(body.to_vec())
            .unwrap()
            .matches("hello")
            .count(),
        1
    );
}

#[forklaunch_core::contract::api]
mod static_operations {
    use super::*;
    #[access(Authenticated)]
    pub fn events() -> Endpoint<NoBody, SseBody<Message>> {
        super::sse()
    }
}

#[tokio::test]
async fn complete_static_sse_binding_uses_managed_frame_validation() {
    let bindings = static_operations::Bindings {
        events: static_operations::Events::bind_sse_authorized(
            Policy::new(),
            options(Shutdown::new()),
            |_, _, ()| async {
                Ok(futures_util::stream::iter([
                    Ok(ServerEvent::new(Message {
                        text: "hello".into(),
                    })),
                    Ok(ServerEvent::new(Message { text: "x".into() })),
                    Ok(ServerEvent::new(Message {
                        text: "must not escape".into(),
                    })),
                ]))
            },
        ),
    };
    let app = bindings
        .mount(ContractRouter::<()>::new(static_operations::registry()))
        .unwrap()
        .finish()
        .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("hello"));
    assert!(!text.contains("must not escape"));
    assert!(!text.contains("\"x\""));
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct StaticHeaders {
    #[serde(rename = "x-sequence")]
    #[schemars(range(min = 1))]
    sequence: u32,
}
#[forklaunch_core::contract::api]
mod static_header_operations {
    use super::*;
    #[access(Authenticated)]
    pub fn events() -> Endpoint<NoBody, WithResponseHeaders<SseBody<Message>, StaticHeaders>> {
        super::sse()
            .with_typed_response_headers::<StaticHeaders>()
            .unwrap()
    }
}
#[tokio::test]
async fn complete_static_sse_binding_validates_typed_headers() {
    for sequence in [7, 0] {
        let bindings = static_header_operations::Bindings {
            events: static_header_operations::Events::bind_sse_authorized_with_headers(
                Policy::new(),
                options(Shutdown::new()),
                move |_, _, ()| async move {
                    Ok(ResponseWithHeaders {
                        body: futures_util::stream::iter([Ok(ServerEvent::new(Message {
                            text: "hello".into(),
                        }))]),
                        headers: StaticHeaders { sequence },
                    })
                },
            ),
        };
        let app = bindings
            .mount(ContractRouter::<()>::new(
                static_header_operations::registry(),
            ))
            .unwrap()
            .finish()
            .unwrap();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if sequence == 7 {
            assert_eq!(response.headers()["x-sequence"], "7");
            assert_eq!(response.status(), StatusCode::OK);
        } else {
            assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        }
    }
}

#[test]
fn ordinary_authentication_cannot_mount_internal_streams_or_sockets() {
    let endpoint = sse().with_access(AccessLevel::Internal);
    let mut registry = ContractRegistry::new();
    registry.register(endpoint.contract());
    let result = ContractRouter::<()>::new(registry).bind_sse_authorized(
        endpoint,
        Policy::new(),
        options(Shutdown::new()),
        |_, _, ()| async {
            Ok(futures_util::stream::empty::<
                Result<ServerEvent<Message>, EndpointError>,
            >())
        },
    );
    assert!(result.is_err());
    let socket = forklaunch_core::ws::TypedSocket::<NoBody, Message, Message>::new(
        RouteContract::get("private", "private", "/private").with_access(AccessLevel::Internal),
        "in",
        "out",
    )
    .unwrap();
    let mut registry = ContractRegistry::new();
    registry
        .register_socket("private", &socket.contract())
        .unwrap();
    let result = ContractRouter::<()>::new(registry).bind_socket_authorized(
        "private",
        socket,
        Policy::new(),
        options(Shutdown::new()),
        |_, _, (), _| async {},
    );
    assert!(result.is_err());
}
