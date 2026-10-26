use super::*;
use axum::{Extension, body::Body, http::Request};
use forklaunch_core::contract::{
    BodyContract, RequestContract, ResponseContract, VersionedContract,
};
use forklaunch_core::http::SelectedVersion;
use serde_json::json;
use tower::ServiceExt;

fn versioned_router() -> ForkLaunchRouter {
    let mut contract = RouteContract::post("Versioned", "Versioned", "/versioned");
    for (name, min, status) in [("z-first", 0, 201), ("a-last", 10, 202)] {
        let mut body = BodyContract::json("Input");
        body.schema=body.schema.with_json_schema(json!({"type":"object","required":["value"],"properties":{"value":{"type":"integer","minimum":min}}}).to_string());
        contract = contract.with_version(VersionedContract {
            version: name.into(),
            request: RequestContract::default().with_body(body),
            responses: vec![ResponseContract::new(status, None)],
        });
    }
    let service = ServiceInfo::new("versioned", "1");
    ForkLaunchRouter::new(service).post(
        contract,
        |Extension(version): Extension<SelectedVersion>| async move {
            (
                if version.version == "a-last" {
                    StatusCode::ACCEPTED
                } else {
                    StatusCode::CREATED
                },
                version.version,
            )
        },
    )
}

#[tokio::test]
async fn versioned_http_dispatch_selects_last_match_and_its_response_contract() {
    let router = versioned_router().into_router();
    for (value, status, name) in [(1, 201, "z-first"), (11, 202, "a-last"), (-1, 400, "")] {
        let response = router
            .clone()
            .oneshot(
                Request::post("/versioned")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({"value":value}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status);
        if status != 400 {
            assert_eq!(to_bytes(response.into_body(), 1024).await.unwrap(), name);
        }
    }
}

#[cfg(feature = "mcp")]
#[tokio::test]
async fn versioned_http_and_mcp_start_discover_invoke_and_shutdown() {
    let app = versioned_router()
        .into_router_with_mcp(forklaunch_core::mcp::server::McpServerOptions::new(
            "versioned",
            "1",
        ))
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
    });
    let client = reqwest::Client::new();
    let base = format!("http://{address}");
    let response = client
        .post(format!("{base}/versioned"))
        .json(&json!({"value":11}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 202);
    assert_eq!(response.text().await.unwrap(), "a-last");
    for (method, params) in [
        (
            "initialize",
            json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"version-test","version":"1"}}),
        ),
        ("tools/list", json!({})),
        (
            "tools/call",
            json!({"name":"versioned [vz-first]","arguments":{"body":{"value":11}}}),
        ),
        (
            "tools/call",
            json!({"name":"versioned [va-last]","arguments":{"body":{"value":11}}}),
        ),
    ] {
        let response = client
            .post(format!("{base}/mcp"))
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", "2025-11-25")
            .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
        let payload: serde_json::Value = response.json().await.unwrap();
        assert!(payload.get("error").is_none(), "{payload}");
        if method == "tools/list" {
            let tools = payload["result"]["tools"].as_array().unwrap();
            assert_eq!(tools.len(), 2);
            assert!(
                tools
                    .iter()
                    .any(|tool| tool["name"] == "versioned [vz-first]")
            );
            assert!(
                tools
                    .iter()
                    .any(|tool| tool["name"] == "versioned [va-last]")
            );
        }
        if method == "tools/call" {
            assert_eq!(payload["result"]["isError"], false, "{payload}");
            assert!(
                payload["result"]["content"].to_string().contains("a-last"),
                "{payload}"
            );
        }
    }
    stop.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn versioned_text_keeps_empty_body_and_uses_the_first_declared_parser() {
    use forklaunch_core::contract::{BodyKind, SchemaRef};
    let mut contract = RouteContract::post("Text", "Text", "/text");
    for (name, min, status) in [("v1", 0, 201), ("v2", 2, 202)] {
        let body = BodyContract {
            kind: BodyKind::Text,
            schema: SchemaRef::named("Text")
                .with_json_schema(json!({"type":"string","minLength":min}).to_string()),
            content_type: Some("text/plain".into()),
        };
        contract = contract.with_version(VersionedContract {
            version: name.into(),
            request: RequestContract::default().with_body(body),
            responses: vec![ResponseContract::new(status, None)],
        });
    }
    let app = ForkLaunchRouter::new(ServiceInfo::new("text", "1"))
        .post(
            contract,
            |Extension(version): Extension<SelectedVersion>, body: axum::body::Bytes| async move {
                (
                    if version.version == "v1" {
                        StatusCode::CREATED
                    } else {
                        StatusCode::ACCEPTED
                    },
                    format!("{}:{}", version.version, String::from_utf8_lossy(&body)),
                )
            },
        )
        .into_router();
    for (body, status, expected) in [("", 201, "v1:"), ("a", 201, "v1:a"), ("水a", 202, "v2:水a")]
    {
        let response = app
            .clone()
            .oneshot(
                Request::post("/text")
                    .header("content-type", "text/plain")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status);
        assert_eq!(
            to_bytes(response.into_body(), 1024).await.unwrap(),
            expected
        );
    }
    let response = app
        .oneshot(
            Request::post("/text")
                .header("content-type", "text/plain")
                .body(Body::from(vec![255]))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    assert_eq!(to_bytes(response.into_body(), 1024).await.unwrap(), "v1:�");
}

#[tokio::test]
async fn versioned_files_preserve_non_utf8_empty_and_json_looking_bytes() {
    let mut contract = RouteContract::post("File", "File", "/file");
    for name in ["v1", "v2"] {
        let body = BodyContract {
            kind: BodyKind::File,
            schema: forklaunch_core::contract::SchemaRef::named("File")
                .with_json_schema(json!({"type":"string","format":"binary"}).to_string()),
            content_type: Some("application/octet-stream".into()),
        };
        contract = contract.with_version(VersionedContract {
            version: name.into(),
            request: RequestContract::default().with_body(body),
            responses: vec![ResponseContract::new(202, None)],
        });
    }
    let app = ForkLaunchRouter::new(ServiceInfo::new("file", "1"))
        .post(
            contract,
            |Extension(version): Extension<SelectedVersion>, body: bytes::Bytes| async move {
                assert_eq!(version.version, "v2");
                assert_eq!(version.matched_versions, ["v1", "v2"]);
                (StatusCode::ACCEPTED, body)
            },
        )
        .into_router();
    for bytes in [
        vec![],
        vec![0, 255, 128],
        b"{\"value\":1}".to_vec(),
        "水".as_bytes().to_vec(),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::post("/file")
                    .header("content-type", "application/octet-stream; charset=binary")
                    .body(Body::from(bytes.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 202);
        assert_eq!(
            to_bytes(response.into_body(), 1024).await.unwrap().as_ref(),
            bytes
        );
    }
}

#[tokio::test]
async fn versioned_multipart_keeps_wire_bytes_and_respects_shared_limits() {
    let mut contract = RouteContract::post("Multipart", "Multipart", "/multipart");
    for (name, minimum, status) in [("v1", 1, 201), ("v2", 2, 202)] {
        let body = BodyContract {
            kind: BodyKind::Multipart,
            schema: forklaunch_core::contract::SchemaRef::named("Multipart").with_json_schema(
                json!({"type":"object","required":["label"],"properties":{"label":{"type":"string","minLength":minimum}}}).to_string()),
            content_type: Some("multipart/form-data".into()),
        };
        contract = contract.with_version(VersionedContract {
            version: name.into(),
            request: RequestContract::default().with_body(body),
            responses: vec![ResponseContract::new(status, None)],
        });
    }
    let app = ForkLaunchRouter::new(ServiceInfo::new("multipart", "1"))
        .post(
            contract,
            |Extension(version): Extension<SelectedVersion>, body: bytes::Bytes| async move {
                (
                    if version.version == "v1" {
                        StatusCode::CREATED
                    } else {
                        StatusCode::ACCEPTED
                    },
                    body,
                )
            },
        )
        .into_router();
    for (text, status) in [("a", 201), ("水a", 202), ("", 400)] {
        let wire = format!(
            "--b\r\nContent-Disposition: form-data; name=\"label\"\r\n\r\n{text}\r\n--b--\r\n"
        );
        let request = || {
            Request::post("/multipart")
                .header("content-type", "multipart/form-data; boundary=b")
                .body(Body::from(wire.clone()))
                .unwrap()
        };
        let response = app.clone().oneshot(request()).await.unwrap();
        assert_eq!(response.status().as_u16(), status);
        if status != 400 {
            assert_eq!(to_bytes(response.into_body(), 1024).await.unwrap(), wire);
        }
        let limited = app
            .clone()
            .layer(Extension(forklaunch_core::http::MultipartLimits {
                parts: 1,
                field_bytes: 1,
                total_bytes: 1,
            }));
        let response = limited.oneshot(request()).await.unwrap();
        assert_eq!(
            response.status().as_u16(),
            if text == "a" {
                201
            } else if text.is_empty() {
                400
            } else {
                413
            }
        );
    }
}

#[tokio::test]
async fn parser_and_transport_limits_remain_fatal_when_validation_is_disabled() {
    let mut route = RouteContract::post("Parse", "Parse", "/parse");
    route.options.request_validation = ValidationMode::None;
    route = route.with_version(VersionedContract {
        version: "v1".into(),
        request: RequestContract::default().with_body(BodyContract::json("NamedInput")),
        responses: vec![],
    });
    let app = ForkLaunchRouter::new(ServiceInfo::new("parse", "1"))
        .with_config(RouterConfig {
            body_limit: 8,
            ..Default::default()
        })
        .post(route, || async { StatusCode::IM_A_TEAPOT })
        .into_router();
    for (wire, status) in [("{", 400), ("0123456789", 413)] {
        let response = app
            .clone()
            .oneshot(
                Request::post("/parse")
                    .header("content-type", "application/json")
                    .body(Body::from(wire))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status);
    }
    let response = body_read_error_response(&axum::Error::new(std::io::Error::other(
        "fixture private I/O failure",
    )));
    assert_eq!(response.status(), 400);
    let bytes = to_bytes(response.into_body(), 1024).await.unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("private"));
}

#[tokio::test]
async fn versioned_body_delivers_parsed_values_without_consuming_wire_bytes() {
    use crate::VersionedBody;
    use forklaunch_core::contract::{BodyKind, SchemaRef};
    let cases = [
        (
            BodyKind::Json,
            "application/json",
            b"\xef\xbb\xbf{\"value\":1}".to_vec(),
            json!({"value":1}),
        ),
        (BodyKind::Json, "application/json", vec![], json!({})),
        (
            BodyKind::Text,
            "text/plain",
            vec![0xef, 0xbb, 0xbf, 255],
            json!("�"),
        ),
        (
            BodyKind::File,
            "application/octet-stream",
            vec![0, 255, 128],
            json!([0, 255, 128]),
        ),
        (
            BodyKind::UrlEncoded,
            "application/x-www-form-urlencoded",
            b"items[]=a&items[]=%E6%B0%B4".to_vec(),
            json!({"items":["a","水"]}),
        ),
        (
            BodyKind::Multipart,
            "multipart/form-data; boundary=b",
            b"--b\r\nContent-Disposition: form-data; name=\"value\"\r\n\r\ntext\r\n--b--\r\n"
                .to_vec(),
            json!({"value":"text"}),
        ),
    ];
    for (kind, content_type, wire, expected) in cases {
        let contract =
            RouteContract::post("Parsed", "Parsed", "/parsed").with_version(VersionedContract {
                version: "v1".into(),
                request: RequestContract::default().with_body(BodyContract {
                    kind,
                    content_type: Some(content_type.into()),
                    schema: SchemaRef::named("Input").with_json_schema("true"),
                }),
                responses: vec![ResponseContract::new(200, None)],
            });
        let app = ForkLaunchRouter::new(ServiceInfo::new("parsed", "1"))
            .post(
                contract,
                |VersionedBody(value): VersionedBody<serde_json::Value>,
                 bytes: axum::body::Bytes| async move {
                    axum::Json(json!({"value":value,"wire":bytes.to_vec()}))
                },
            )
            .into_router();
        let response = app
            .oneshot(
                Request::post("/parsed")
                    .header("content-type", content_type)
                    .body(Body::from(wire.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let result: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
        assert_eq!(status, 200, "{kind:?}: {result}");
        assert_eq!(result, json!({"value":expected,"wire":wire}));
    }
}

#[tokio::test]
async fn native_body_hooks_run_once_through_direct_and_nested_local_adapters() {
    use forklaunch_core::http::{VersionBodyHooks, VersionBodyHooksRegistry};
    for (local, trailing, root) in [
        (false, false, false),
        (false, true, false),
        (true, false, false),
        (true, true, false),
        (false, false, true),
        (false, true, true),
        (true, false, true),
        (true, true, true),
    ] {
        let route_path = if root { "/" } else { "/hook" };
        let wire_path = match (root, trailing) {
            (false, false) => "/api/hook",
            (false, true) => "/api/hook/",
            (true, false) => "/api",
            (true, true) => "/api/",
        };
        let mut contract = RouteContract::post("Hook", "Hook", route_path);
        contract.options.body_parser.json_strict = false;
        contract = contract.with_version(VersionedContract {
            version: "one".into(),
            request: RequestContract::default().with_body(BodyContract {
                schema: forklaunch_core::contract::SchemaRef::named("Number")
                    .with_json_schema(r#"{"type":"integer"}"#),
                ..BodyContract::json("Number")
            }),
            responses: vec![ResponseContract::new(201, None)],
        });
        let calls = Arc::new(AtomicU64::new(0));
        let count = calls.clone();
        let mut hooks = VersionBodyHooksRegistry::default();
        hooks
            .insert(
                &contract,
                "one",
                VersionBodyHooks::default()
                    .with_preprocess(move |value| {
                        count.fetch_add(1, Ordering::SeqCst);
                        value
                            .as_str()
                            .ok_or("expected string")?
                            .parse::<i64>()
                            .map(serde_json::Value::from)
                            .map_err(|error| error.to_string())
                    })
                    .with_transform(|value| Ok(json!({"result":value.as_i64().unwrap()+1}))),
            )
            .unwrap();
        let service = ServiceInfo::new("hooks", "1");
        let mut child = if local {
            let mut handlers = LocalRouter::new();
            handlers.register_with_body_hooks(
                contract.clone(),
                &hooks,
                move |request: FrameworkRequest| {
                    assert_eq!(request.raw_path.as_deref(), Some(wire_path));
                    assert_eq!(request.version.as_ref().unwrap().version, "one");
                    assert_eq!(request.body, FrameworkBody::Text("\"41\"".into()));
                    Ok(FrameworkResponse::json(
                        201,
                        request.version.unwrap().parsed_body.unwrap().to_string(),
                    ))
                },
            );
            ForkLaunchRouter::from_local_router(service.clone(), [contract], handlers)
        } else {
            ForkLaunchRouter::new(service.clone()).with_version_body_hooks(hooks.clone()).post(contract,
                |crate::VersionedBody(value): crate::VersionedBody<serde_json::Value>| async move {
                    (StatusCode::CREATED, axum::Json(value))
                })
        };
        child
            .config
            .optional_trailing_slash_paths
            .insert(route_path.into());
        // Hook registration survives mounting a child router, and local selection
        // sees its original route path rather than the flattened transport prefix.
        let app = ForkLaunchRouter::new(service)
            .nest("/api", child)
            .into_router();
        let response = app
            .oneshot(
                Request::post(wire_path)
                    .header("content-type", "application/json")
                    .body(Body::from("\"41\""))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::CREATED,
            "local={local}, trailing={trailing}, root={root}"
        );
        let value: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1024).await.unwrap()).unwrap();
        assert_eq!(value, json!({"result":42}));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn metadata_adapter_delivers_transformed_body_and_keeps_wire_bytes() {
    use forklaunch_core::http::{HttpApplication, VersionBodyHooks, VersionBodyHooksRegistry};
    let contract = RouteContract::post("Hook", "Hook", "/hook").with_version(VersionedContract {
        version: "one".into(),
        request: RequestContract::default().with_body(BodyContract::json("Input")),
        responses: vec![ResponseContract::new(200, None)],
    });
    let mut hooks = VersionBodyHooksRegistry::default();
    hooks
        .insert(
            &contract,
            "one",
            VersionBodyHooks::default().with_transform(|value| {
                Ok(json!({"result":value["value"].as_i64().ok_or("missing value")?+1}))
            }),
        )
        .unwrap();
    let mut app = HttpApplication::new(ServiceInfo::new("hooks", "1"));
    app.register_route(contract);
    let mut adapter = crate::AxumApplication::new(app);
    adapter.version_body_hooks = hooks;
    let prepared = adapter.prepare_request(
        crate::AxumRequestParts::new(HttpMethod::Post, "/hook")
            .with_header("content-type", "application/json")
            .with_body(br#"{"value":41}"#.to_vec()),
    );
    assert!(prepared.validation_errors.is_empty());
    assert_eq!(
        prepared.request.body,
        FrameworkBody::Text(r#"{"value":41}"#.into())
    );
    assert_eq!(
        *prepared.request.version.unwrap().parsed_body.unwrap(),
        json!({"result":42})
    );
}

#[tokio::test]
async fn parser_verification_is_once_only_and_fatal_in_all_validation_modes() {
    use forklaunch_core::http::{BodyParserHooks, VersionBodyHooksRegistry};
    for mode in [
        ValidationMode::Error,
        ValidationMode::Warning,
        ValidationMode::None,
    ] {
        for local in [false, true] {
            let mut contract = RouteContract::post("Verify", "Verify", "/verify");
            contract.options.request_validation = mode;
            contract.options.body_parser.json_wire.limit_bytes = 2;
            for version in ["first", "last"] {
                contract = contract.with_version(VersionedContract {
                    version: version.into(),
                    request: RequestContract::default().with_body(BodyContract::json("Input")),
                    responses: vec![ResponseContract::new(200, None)],
                });
            }
            let count = Arc::new(AtomicU64::new(0));
            let calls = count.clone();
            let mut hooks = VersionBodyHooksRegistry::default();
            hooks
                .insert_parser(
                    &contract,
                    BodyParserHooks::default().with_verify(move |request, bytes, charset| {
                        calls.fetch_add(1, Ordering::SeqCst);
                        assert_eq!(charset, Some("utf-8"));
                        if request.headers.get("x-deny").map(String::as_str) == Some("yes") {
                            return Err("private verification detail".into());
                        }
                        assert_eq!(bytes, b"{}");
                        Ok(())
                    }),
                )
                .unwrap();
            let service = ServiceInfo::new("verify", "1");
            let router = if local {
                let mut handlers = LocalRouter::new();
                handlers.register_with_body_hooks(
                    contract.clone(),
                    &hooks,
                    |_: FrameworkRequest| Ok(FrameworkResponse::new(200)),
                );
                ForkLaunchRouter::from_local_router(service, [contract], handlers)
            } else {
                ForkLaunchRouter::new(service)
                    .with_version_body_hooks(hooks)
                    .post(contract, || async { StatusCode::OK })
            }
            .into_router();
            for (body, deny, expected) in
                [("{}", "no", 200), ("{", "yes", 403), ("{} ", "yes", 413)]
            {
                let response = router
                    .clone()
                    .oneshot(
                        Request::post("/verify")
                            .header("content-type", "application/json")
                            .header("x-deny", deny)
                            .body(Body::from(body))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    response.status().as_u16(),
                    expected,
                    "{mode:?}, local={local}"
                );
                let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
                assert!(!String::from_utf8_lossy(&bytes).contains("private verification detail"));
            }
            assert_eq!(count.load(Ordering::SeqCst), 2);
        }
    }
}

fn parser_profile_router(
    format: ParserErrorFormat,
    mode: ValidationMode,
    local: bool,
) -> (Router, Arc<AtomicU64>) {
    use forklaunch_core::http::{BodyParserHooks, VersionBodyHooksRegistry};
    let mut contract = RouteContract::post("Parser Profile", "Parser Profile", "/parser")
        .with_version(VersionedContract {
            version: "v1".into(),
            request: RequestContract::default().with_body(BodyContract::json("Input")),
            responses: vec![ResponseContract::new(200, None)],
        });
    contract.options.request_validation = mode;
    contract.options.body_parser.json_wire.limit_bytes = 64;
    let mut hooks = VersionBodyHooksRegistry::default();
    hooks
        .insert_parser(
            &contract,
            BodyParserHooks::default().with_verify(|request, _, _| {
                if request.headers.contains_key("x-deny") {
                    Err("PRIVATE_VERIFIER".into())
                } else {
                    Ok(())
                }
            }),
        )
        .unwrap();
    let count = Arc::new(AtomicU64::new(0));
    let calls = Arc::clone(&count);
    let router = if local {
        let mut handlers = LocalRouter::new();
        handlers.register_with_body_hooks(contract.clone(), &hooks, move |_: FrameworkRequest| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(FrameworkResponse::new(200))
        });
        ForkLaunchRouter::from_local_router(ServiceInfo::new("parser", "1"), [contract], handlers)
    } else {
        ForkLaunchRouter::new(ServiceInfo::new("parser", "1")).post(contract, move || {
            let calls = Arc::clone(&calls);
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                StatusCode::OK
            }
        })
    }
    .with_config(RouterConfig {
        parser_error_format: format,
        version_body_hooks: hooks,
        body_limit: 128,
        ..RouterConfig::default()
    })
    .into_router();
    (router, count)
}

#[tokio::test]
async fn parser_error_profiles_preserve_status_redaction_and_fatal_limits() {
    for format in [ParserErrorFormat::Json, ParserErrorFormat::TypeScriptText] {
        for mode in [
            ValidationMode::Error,
            ValidationMode::Warning,
            ValidationMode::None,
        ] {
            for local in [false, true] {
                let (router, count) = parser_profile_router(format, mode, local);
                for (body, charset, deny, length, status) in [
                    ("null".to_owned(), false, false, false, 400),
                    ("{\"PRIVATE_BODY\":".to_owned(), false, false, false, 400),
                    ("x".repeat(80), false, false, false, 413),
                    ("x".repeat(160), false, false, false, 413),
                    ("x".repeat(160), false, false, true, 413),
                    ("{}".to_owned(), true, false, false, 415),
                    ("{}".to_owned(), false, true, false, 403),
                ] {
                    let mut request = Request::post("/parser")
                        .header(
                            "content-type",
                            if charset {
                                "application/json; charset=unsupported"
                            } else {
                                "application/json"
                            },
                        )
                        .header("x-correlation-id", "parser-correlation");
                    if deny {
                        request = request.header("x-deny", "yes");
                    }
                    if length {
                        request = request.header("content-length", body.len().to_string());
                    }
                    let response = router
                        .clone()
                        .oneshot(request.body(Body::from(body)).unwrap())
                        .await
                        .unwrap();
                    assert_eq!(
                        response.status().as_u16(),
                        status,
                        "local={local} format={format:?} mode={mode:?}"
                    );
                    let media_type = response
                        .headers()
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_owned();
                    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
                    let text = std::str::from_utf8(&bytes).unwrap();
                    assert!(!text.contains("PRIVATE"));
                    assert_eq!(count.load(Ordering::SeqCst), 0);
                    if format == ParserErrorFormat::TypeScriptText {
                        assert_eq!(media_type, "text/plain; charset=utf-8");
                        assert_eq!(
                            text,
                            "Request could not be processed:\n\nCorrelation id: parser-correlation"
                        );
                    }
                }
                let response = router
                    .oneshot(
                        Request::post("/parser")
                            .header("content-type", "application/json")
                            .body(Body::from("{}"))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), 200);
                assert_eq!(count.load(Ordering::SeqCst), 1);
            }
        }
    }
}

#[tokio::test]
async fn uri_error_profile_matches_pre_context_failures_without_rewriting_native_errors() {
    for format in [ParserErrorFormat::Json, ParserErrorFormat::TypeScriptText] {
        for local in [false, true] {
            let contract = RouteContract::get("URI", "URI", "/items/:id")
                .with_param(forklaunch_core::contract::FieldContract::required(
                    "id", "String",
                ))
                .with_response(200, None);
            let count = Arc::new(AtomicU64::new(0));
            let calls = Arc::clone(&count);
            let router = if local {
                let mut handlers = LocalRouter::new();
                handlers.register(contract.clone(), move |_: FrameworkRequest| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(FrameworkResponse::new(200))
                });
                ForkLaunchRouter::from_local_router(
                    ServiceInfo::new("uri", "1"),
                    [contract],
                    handlers,
                )
            } else {
                ForkLaunchRouter::new(ServiceInfo::new("uri", "1")).get(contract, move || {
                    let calls = Arc::clone(&calls);
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        StatusCode::OK
                    }
                })
            }
            .with_config(RouterConfig {
                parser_error_format: format,
                ..RouterConfig::default()
            })
            .into_router();
            for suffix in ["%FF", "%ZZ", "%C0%AF", "%E0%A4%A"] {
                let response = router
                    .clone()
                    .oneshot(
                        Request::get(format!("/items/{suffix}"))
                            .header("x-correlation-id", "native-correlation")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), 400);
                let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
                let text = std::str::from_utf8(&bytes).unwrap();
                if format == ParserErrorFormat::TypeScriptText {
                    assert_eq!(
                        text,
                        "Request could not be processed:\n\nCorrelation id: No correlation ID"
                    );
                } else {
                    let value: serde_json::Value = serde_json::from_str(text).unwrap();
                    assert_eq!(value["code"], "validation_error");
                    assert_eq!(value["correlationId"], "native-correlation");
                }
                assert_eq!(count.load(Ordering::SeqCst), 0);
            }
            for suffix in ["user_1", "%75ser_1", "%E6%B0%B4"] {
                let response = router
                    .clone()
                    .oneshot(
                        Request::get(format!("/items/{suffix}"))
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), 200);
            }
            assert_eq!(count.load(Ordering::SeqCst), 3);
        }
    }
}

#[cfg(feature = "zod-regex")]
#[tokio::test]
async fn checked_regex_exhaustion_is_fatal_on_both_http_adapters() {
    for mode in [
        ValidationMode::None,
        ValidationMode::Warning,
        ValidationMode::Error,
    ] {
        for local in [false, true] {
            let schema = json!({"type":"object","required":["value"],"properties":{"value":{
                "type":"string","x-forklaunch-zod-regex":{"version":1,"rules":[{
                    "sourceUnits":"^(a+)+$".encode_utf16().collect::<Vec<_>>(),"flags":""
                }]}
            }}});
            let mut body = BodyContract::json("Input");
            body.schema = body.schema.with_json_schema(schema.to_string());
            let mut contract =
                RouteContract::post("Regex", "Regex", "/regex").with_version(VersionedContract {
                    version: "checked".into(),
                    request: RequestContract::default().with_body(body),
                    responses: vec![ResponseContract::new(200, None)],
                });
            contract.options.request_validation = mode;
            let count = Arc::new(AtomicU64::new(0));
            let calls = Arc::clone(&count);
            let router = if local {
                let mut handlers = LocalRouter::new();
                handlers.register(contract.clone(), move |_: FrameworkRequest| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(FrameworkResponse::new(200))
                });
                ForkLaunchRouter::from_local_router(
                    ServiceInfo::new("regex", "1"),
                    [contract],
                    handlers,
                )
            } else {
                ForkLaunchRouter::new(ServiceInfo::new("regex", "1")).post(contract, move || {
                    let calls = Arc::clone(&calls);
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        StatusCode::OK
                    }
                })
            }
            .into_router();
            for (value, status) in [(format!("{}!", "a".repeat(40)), 500), ("a".into(), 200)] {
                let response = router
                    .clone()
                    .oneshot(
                        Request::post("/regex")
                            .header("content-type", "application/json")
                            .header("x-correlation-id", "regex-correlation")
                            .body(Body::from(json!({"value":value}).to_string()))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status().as_u16(), status);
                if status == 500 {
                    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
                    let text = std::str::from_utf8(&bytes).unwrap();
                    assert!(text.contains("regex-correlation"));
                    assert!(!text.contains(&"a".repeat(40)));
                    assert_eq!(count.load(Ordering::SeqCst), 0);
                }
            }
            assert_eq!(count.load(Ordering::SeqCst), 1);
        }
    }
}
