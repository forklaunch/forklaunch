use super::*;
use axum::{body::to_bytes, http::Request};
use forklaunch_core::contract::BodyContract;
use serde_json::{Value, json};

fn application(config: RouterConfig) -> ForkLaunchRouter {
    let mut body = BodyContract::json("Note");
    body.schema = body.schema.with_json_schema(
        r#"{"type":"object","required":["title"],"additionalProperties":false,"properties":{"title":{"type":"string"}}}"#,
    );
    let mut private = RouteContract::get("Private", "Private", "/internal");
    private.options.openapi = false;
    ForkLaunchRouter::new(ServiceInfo::new("notes", "1.2.3"))
        .with_config(config)
        .get(
            RouteContract::get("Read", "Read note", "/notes").with_response(200, Some(body)),
            || async { axum::Json(json!({"title":"Example"})) },
        )
        .get(private, || async { "private" })
}

async fn read(router: &Router, path: &str) -> (StatusCode, String, String) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(path)
                .body(AxumBody::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let body = String::from_utf8(
        to_bytes(response.into_body(), 128 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    (status, content_type, body)
}

#[tokio::test]
async fn adapter_discovery_uses_checked_public_descriptors_and_stable_document_hash() {
    let app = application(RouterConfig::default());
    let registry = app.registry().clone();
    let expected = forklaunch_core::openapi::generate_portable_openapi_json(&registry).unwrap();
    let plan = forklaunch_core::codegen::SdkGenerationPlan::from_registry(
        forklaunch_core::codegen::SdkTarget::TypeScript,
        &registry,
    );
    let declarations = forklaunch_core::typescript::generate_typescript_service_declarations(
        &plan,
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("notes"),
    )
    .unwrap();
    let router = app.into_router();
    let (status, content_type, document) = read(&router, "/openapi.json").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type, "application/json");
    assert_eq!(document, expected);
    let document_json: Value = serde_json::from_str(&document).unwrap();
    assert!(document_json["paths"].get("/internal").is_none());
    let operation = &document_json["paths"]["/notes"]["get"]["x-forklaunch-operation"];
    assert_eq!(operation["format"], "forklaunch.sdk-operation.v1");
    let client_path = operation["clientPath"].as_array().unwrap();
    assert_eq!(client_path.len(), 2);
    for segment in client_path {
        assert!(declarations.contains(&format!("{}:", segment.as_str().unwrap())));
    }
    let (status, content_type, fingerprint) = read(&router, "/openapi.json-hash").await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/plain"));
    assert_eq!(fingerprint, crate::docs::document_hash(&document));
    assert_eq!(read(&router, "/openapi.json-hash").await.2, fingerprint);
    assert_ne!(fingerprint, crate::docs::document_hash(&(document + " ")));

    #[cfg(feature = "native")]
    {
        let native =
            crate::native::with_openapi(Router::new(), &registry, "/reference", "v1").unwrap();
        assert_eq!(read(&native, "/openapi.json-hash").await.2, fingerprint);
    }
}

#[tokio::test]
async fn adapter_discovery_preserves_configured_paths_uis_and_openapi_options() {
    let config = RouterConfig {
        openapi_path: "/api/schema".into(),
        docs_path: "/reference".into(),
        docs_title: "Notes reference".into(),
        swagger_enabled: true,
        swagger_path: "/explorer".into(),
        description: Some("Application description".into()),
        servers: vec![OpenApiServer::new("https://notes.example").with_description("Notes API")],
        ..RouterConfig::default()
    };
    let app = application(config.clone());
    let expected = generate_portable_openapi_json_with_options(
        app.registry(),
        &config.openapi_options(app.registry()),
    )
    .unwrap();
    let router = app.into_router();
    let document = read(&router, "/api/schema").await;
    assert_eq!(document.0, StatusCode::OK);
    assert_eq!(document.2, expected);
    let value: Value = serde_json::from_str(&document.2).unwrap();
    assert_eq!(value["info"]["description"], "Application description");
    assert_eq!(
        value["servers"],
        json!([{"url":"https://notes.example","description":"Notes API"}])
    );
    assert_eq!(
        read(&router, "/api/schema-hash").await.2,
        crate::docs::document_hash(&document.2)
    );
    for (path, ui) in [
        ("/reference", "Scalar.createApiReference"),
        ("/explorer", "SwaggerUIBundle"),
    ] {
        let (status, content_type, html) = read(&router, path).await;
        assert_eq!(status, StatusCode::OK);
        assert!(content_type.starts_with("text/html"));
        assert!(html.contains(ui));
        assert!(html.contains("/api/schema"));
        assert!(html.contains("Notes reference"));
    }
    assert_eq!(
        read(&router, "/openapi.json").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        read(&router, "/openapi.json-hash").await.0,
        StatusCode::NOT_FOUND
    );
}

#[test]
fn adapter_discovery_reserves_document_hash_against_contracts_and_internal_mounts() {
    for path in ["/openapi.json", "/openapi.json-hash"] {
        for method in [HttpMethod::Get, HttpMethod::Head] {
            let app = ForkLaunchRouter::anonymous().route(
                RouteContract::new("Conflict", "Conflict", method, path),
                || async { "application" },
            );
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.into_router()))
                    .is_err()
            );
        }
        let app = ForkLaunchRouter::anonymous().nest_internal(path, Router::new());
        assert!(internal::validate(&app.internal_mounts, app.registry(), &app.config).is_err());
    }
}

#[cfg(feature = "mcp")]
#[test]
fn adapter_discovery_reserves_configured_document_hash_against_mcp() {
    for path in ["/schema", "/schema-hash"] {
        let config = RouterConfig {
            openapi_path: "/schema".into(),
            ..RouterConfig::default()
        };
        let mut options = forklaunch_core::mcp::server::McpServerOptions::new("notes", "1");
        options.path = path.into();
        assert!(
            matches!(application(config).into_routers_with_mcp(options), Err(message) if message == "MCP endpoint conflicts with documentation")
        );
    }
}

#[tokio::test]
async fn disabled_documentation_does_not_claim_application_hash_paths() {
    let config = RouterConfig {
        openapi_enabled: false,
        ..RouterConfig::default()
    };
    let app = ForkLaunchRouter::anonymous()
        .with_config(config)
        .get(
            RouteContract::get("Application", "Application", "/openapi.json-hash"),
            || async { "application" },
        )
        .into_router();
    assert_eq!(read(&app, "/openapi.json-hash").await.2, "application");
    assert_eq!(read(&app, "/openapi.json").await.0, StatusCode::NOT_FOUND);
    assert_eq!(read(&app, "/docs").await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn adapter_discovery_preserves_optional_schema_contracts() {
    let router = ForkLaunchRouter::new(ServiceInfo::new("registry-smoke", "0.1.0"))
        .get(
            RouteContract::get("Health", "Health", "/health")
                .with_response(200, Some(BodyContract::json("Health"))),
            || async { axum::Json(json!({"ok":true})) },
        )
        .into_router();
    assert_eq!(read(&router, "/health").await.0, StatusCode::OK);
    let (status, _, document) = read(&router, "/openapi.json").await;
    assert_eq!(status, StatusCode::OK);
    let value: Value = serde_json::from_str(&document).unwrap();
    assert_eq!(
        value["paths"]["/health"]["get"]["x-forklaunch-operation"]["format"],
        "forklaunch.sdk-operation.v1"
    );
    assert_eq!(read(&router, "/openapi.json-hash").await.0, StatusCode::OK);
}

#[tokio::test]
async fn runtime_only_contracts_keep_http_and_docs_but_reject_sdk_discovery() {
    use forklaunch_core::contract::{RequestContract, ResponseContract, VersionedContract};
    let route = RouteContract::post("Runtime", "Runtime-only version predicate", "/runtime")
        .with_version(VersionedContract {
            version: "v1".into(),
            request: RequestContract::default().with_body(BodyContract::json("RuntimeInput")),
            responses: vec![ResponseContract::new(204, None)],
        });
    let app = application(RouterConfig::default()).post(route, || async { StatusCode::NO_CONTENT });
    let expected = generate_openapi_json_with_options(
        app.registry(),
        &app.config.openapi_options(app.registry()),
    );
    assert!(
        generate_portable_openapi_json_with_options(
            app.registry(),
            &app.config.openapi_options(app.registry())
        )
        .is_err()
    );
    let router = app.into_router();
    assert_eq!(read(&router, "/notes").await.0, StatusCode::OK);
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/runtime")
                .header(header::CONTENT_TYPE, "application/json")
                .body(AxumBody::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let (status, _, document) = read(&router, "/openapi.json").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(document, expected);
    assert!(
        !document.contains("x-forklaunch-operation"),
        "No partial checked descriptors may be published"
    );
    let html = read(&router, "/docs").await;
    assert_eq!(html.0, StatusCode::OK);
    assert!(html.2.contains("Scalar.createApiReference"));
    let (status, _, message) = read(&router, "/openapi.json-hash").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(message, "Contract cannot produce checked SDK documentation");
    assert!(!message.contains("RuntimeInput"));
}
