use super::*;
use crate::MockProcedureCodeProvider;
use axum::{body::Body as AxumBody, http::Request};
use forklaunch_core::auth::{jwt::RemoteJwks, request_hmac::SignedBody};
use forklaunch_core::auth::{jwt::RemoteJwksConfig, request_hmac::generate_hmac_auth_header};
use jsonwebtoken::{Algorithm, Validation};
use std::time::SystemTime;
use tower::ServiceExt;
const SECRET: &[u8] = b"cac-test-hmac-secret";
fn jwks() -> RemoteJwks {
    RemoteJwks::new(RemoteJwksConfig::new("http://127.0.0.1:9/jwks")).unwrap()
}
fn router() -> axum::Router {
    let http = CacHttp::new(
        Arc::new(MockProcedureCodeProvider::default()),
        HmacJwksAuth::new(SECRET.to_vec(), jwks(), Validation::new(Algorithm::RS256)).unwrap(),
        ServiceInfo::new("cac", "0.1.0"),
    );
    let routes = contracts::module_registry()
        .routes()
        .iter()
        .fold(http.routes(), |routes, route| {
            routes.route(&route.operation_name, http.handler(&route.operation_name))
        });
    routes.finish().into_router()
}
fn header(path: &str) -> String {
    let timestamp = chrono::DateTime::<chrono::Utc>::from(SystemTime::now())
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    generate_hmac_auth_header(
        SECRET,
        "default",
        "GET",
        path,
        SignedBody::Absent,
        &timestamp,
        "test-nonce",
    )
    .unwrap()
}
#[tokio::test]
async fn lookup_routes_require_valid_request_bound_hmac_and_keep_health_public() {
    let app = router();
    for path in ["/codeSet", "/codeSet/PROC-001", "/compliance/export/user"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .body(AxumBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let health = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(AxumBody::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
    for (path, expected, value) in [
        ("/codeSet", StatusCode::OK, "\"licensed\":false"),
        ("/codeSet/PROC-001", StatusCode::OK, "Office Visit"),
        (
            "/codeSet/unknown",
            StatusCode::NOT_FOUND,
            "Procedure code 'unknown' not found",
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header("authorization", header(path))
                    .body(AxumBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&body).contains(value));
    }
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/codeSet/PROC-002")
                .header("authorization", header("/codeSet/PROC-001"))
                .body(AxumBody::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let stale = generate_hmac_auth_header(
        SECRET,
        "default",
        "GET",
        "/codeSet",
        SignedBody::Absent,
        "2020-01-01T00:00:00Z",
        "old-nonce",
    )
    .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/codeSet")
                .header("authorization", stale)
                .body(AxumBody::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
