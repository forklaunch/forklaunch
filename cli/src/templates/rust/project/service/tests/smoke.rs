use {{crate_module}}::{
    DEFAULT_OPENAPI_PATH, axum_application, build_state, config::ServiceConfig, contract_registry,
    health_check, local_router, openapi_json_with_config, route_names,
};
use forklaunch_core::contract::HttpMethod;
use forklaunch_core::http::{Body, FrameworkRequest, RequestContext};

#[test]
fn generated_service_builds_state_and_reports_health() {
    let state = build_state(ServiceConfig::default()).expect("state should build");
    let health = health_check(&state);

    assert_eq!(health.status, "ok");
    assert_eq!(health.record_count, 1);
    assert!(route_names().contains(&"health"));
}

#[test]
fn generated_service_exposes_framework_contracts() {
    let registry = contract_registry();
    let artifact = registry.clone().into_artifact().to_json();
    let axum = axum_application();

    assert!(artifact.contains("framework-rs"));
    assert!(artifact.contains("forklaunch.contract.v1"));
    assert!(registry.routes().iter().any(|route| route.path == "/health"));
    assert_eq!(axum.service_name(), "{{crate_module}}".replace('_', "-"));
}

#[test]
fn generated_service_serves_openapi_json() {
    let state = build_state(ServiceConfig {
        docs_path: "/docs".to_string(),
        ..ServiceConfig::default()
    })
    .expect("state should build");
    let router = local_router(state);
    let response = router
        .execute(FrameworkRequest::new(
            HttpMethod::Get,
            DEFAULT_OPENAPI_PATH,
            RequestContext::new("test-request"),
        ))
        .expect("OpenAPI route should execute");

    assert_eq!(response.status_code, 200);
    assert_eq!(
        response.headers.get("content-type").map(String::as_str),
        Some("application/json")
    );
    let Body::Text(body) = response.body else {
        panic!("OpenAPI response should be text JSON");
    };
    assert!(body.contains("\"openapi\":\"3.1.0\""));
    assert!(body.contains("/health"));
    assert!(!body.contains(DEFAULT_OPENAPI_PATH));
}

#[test]
fn generated_service_serves_configured_docs_path() {
    let state = build_state(ServiceConfig {
        docs_path: "/docs".to_string(),
        ..ServiceConfig::default()
    })
    .expect("state should build");
    let router = local_router(state);
    let response = router
        .execute(FrameworkRequest::new(
            HttpMethod::Get,
            "/docs",
            RequestContext::new("test-request"),
        ))
        .expect("configured docs route should execute");

    assert_eq!(response.status_code, 200);
    assert_eq!(
        response.headers.get("content-type").map(String::as_str),
        Some("application/json")
    );
}

#[test]
fn generated_service_uses_configured_openapi_servers() {
    let body = openapi_json_with_config(&ServiceConfig {
        protocol: "https".to_string(),
        host: "api.example.com".to_string(),
        port: 443,
        docs_server_urls: vec![
            "https://api.example.com".to_string(),
            "https://staging.example.com".to_string(),
        ],
        docs_server_descriptions: vec!["Production".to_string(), "Staging".to_string()],
        ..ServiceConfig::default()
    });

    assert!(body.contains("\"servers\""));
    assert!(body.contains("\"url\":\"https://api.example.com\""));
    assert!(body.contains("\"description\":\"Production\""));
    assert!(body.contains("\"url\":\"https://staging.example.com\""));
    assert!(body.contains("\"description\":\"Staging\""));
}

#[tokio::test]
async fn generated_service_serves_http_health_docs_and_errors() {
    use axum::{body::{Body as AxumBody, to_bytes}, http::Request};
    use tower::ServiceExt;
    let state = build_state(ServiceConfig::default()).expect("state should build");
    let app = {{crate_module}}::router(state).into_router();
    for (path, status, content_type) in [
        ("/health", 200, "application/json"),
        ("/openapi.json", 200, "application/json"),
        ("/docs", 200, "text/html"),
        ("/missing", 404, ""),
    ] {
        let response = app.clone().oneshot(Request::builder()
            .uri(path).body(AxumBody::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status().as_u16(), status, "{path}");
        if !content_type.is_empty() {
            assert!(response.headers()["content-type"].to_str().unwrap().starts_with(content_type));
            assert!(!to_bytes(response.into_body(), 1024 * 1024).await.unwrap().is_empty());
        }
    }
}
