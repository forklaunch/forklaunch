use axum::{body::{Body, to_bytes}, http::Request};
use std::{env, sync::Once};
use tower::ServiceExt;

/// Settings `register` requires at startup that these routes never use, such
/// as the JWKS URL an added agent reads; the placeholder is never contacted.
fn test_environment() {
    static SET: Once = Once::new();
    SET.call_once(|| {
        if env::var_os("JWKS_PUBLIC_KEY_URL").is_none() {
            // SAFETY: set once, before any test reads the setting.
            unsafe { env::set_var("JWKS_PUBLIC_KEY_URL", "http://127.0.0.1:9/jwks") };
        }
    });
}
#[test]
fn explicit_public_origin_survives_ephemeral_listener_binding() {
    let mut config = {{crate_module}}::config::ServiceConfig {
        port: 0,
        public_origin: Some("https://notes.example".into()),
        docs_server_urls: vec!["http://127.0.0.1:0".into(), "https://docs.example".into()],
        ..Default::default()
    };
    config.set_bound_port(43210);
    assert_eq!(config.port, 43210);
    assert_eq!(config.origin(), "https://notes.example");
    assert_eq!(config.docs_server_urls, ["http://127.0.0.1:43210", "https://docs.example"]);
    config.public_origin = None;
    assert_eq!(config.origin(), "http://127.0.0.1:43210");
}
#[tokio::test]
async fn health_and_contract_surfaces_share_native_router() {
    test_environment();
    let state = {{crate_module}}::register({{crate_module}}::config::ServiceConfig::default()).unwrap();
    let router = {{crate_module}}::server::router(state).unwrap();
    for path in ["/health", "/openapi.json"] {
        let response = router.clone().oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), 200);
        assert!(!to_bytes(response.into_body(), 1024 * 1024).await.unwrap().is_empty());
    }
    for (path, status, content_type) in [
        ("/api/v1/docs", 200, "text/html"),
        ("/api/v1/openapi", 200, "application/json"),
        ("/docs", 404, ""),
    ] {
        let response = router.clone().oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), status, "{path}");
        if status == 200 {
            assert!(response.headers()["content-type"].to_str().unwrap().starts_with(content_type));
        }
    }
    let denied = router.oneshot(Request::builder().method("POST").uri("/mcp")
        .header("origin", "https://untrusted.example").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(denied.status(), 403);
}

#[tokio::test]
async fn documentation_uses_configured_version_and_suffix() {
    test_environment();
    let config = {{crate_module}}::config::ServiceConfig {
        version: "v2".into(), docs_path: "/reference".into(), ..Default::default()
    };
    let state = {{crate_module}}::register(config).unwrap();
    let router = {{crate_module}}::server::router(state).unwrap();
    for (path, expected) in [("/api/v2/reference", 200), ("/api/v1/docs", 404), ("/reference", 404)] {
        let response = router.clone().oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), expected, "{path}");
    }
}
