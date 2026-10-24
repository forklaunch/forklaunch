use super::*;
use crate::{ForkLaunchRouter, RouterConfig};
use axum::{
    body::{Body as AxumBody, to_bytes},
    http::Request,
};
use forklaunch_core::ServiceInfo;
use forklaunch_core::auth::{jwt::RemoteJwksConfig, request_hmac::generate_hmac_auth_header};
use forklaunch_core::contract::{HmacAuth, RouteContract};
use forklaunch_core::http::{FrameworkResponse, LocalRouter};
use tower::ServiceExt;
const SECRET: &[u8] = b"head-hmac-fixture-secret";
fn route() -> RouteContract {
    RouteContract::get("Read", "Read", "/read")
        .with_auth(AuthConfig::Hmac(HmacAuth::new()))
        .with_response(200, None)
}
fn sign(method: &str) -> String {
    let now = chrono::DateTime::<chrono::Utc>::from(SystemTime::now())
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    generate_hmac_auth_header(
        SECRET,
        "default",
        method,
        "/read",
        SignedBody::Absent,
        &now,
        "nonce",
    )
    .unwrap()
}
fn config() -> RouterConfig {
    let auth = Arc::new(
        HmacJwksAuth::new(
            SECRET.to_vec(),
            RemoteJwks::new(RemoteJwksConfig::new("http://127.0.0.1:9/jwks")).unwrap(),
            Validation::new(Algorithm::RS256),
        )
        .unwrap(),
    );
    RouterConfig::default().with_authenticator(move |request, contract| {
        let auth = Arc::clone(&auth);
        async move { auth.authenticate(request, contract).await }
    })
}
#[tokio::test]
async fn implicit_head_signs_head_for_native_and_local_handlers() {
    let native = ForkLaunchRouter::anonymous()
        .get(route(), || async { "response" })
        .with_config(config())
        .into_router();
    let mut local = LocalRouter::new();
    local.register(route(), |request: FrameworkRequest| {
        assert!(matches!(
            request.method,
            forklaunch_core::contract::HttpMethod::Get
                | forklaunch_core::contract::HttpMethod::Head
        ));
        Ok(FrameworkResponse::text(200, "response"))
    });
    let local =
        ForkLaunchRouter::from_local_router(ServiceInfo::new("head", "1"), [route()], local)
            .with_config(config())
            .into_router();
    for app in [native, local] {
        for (method, signature, expected) in [
            ("HEAD", "HEAD", 200),
            ("HEAD", "GET", 401),
            ("GET", "GET", 200),
            ("GET", "HEAD", 401),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri("/read")
                        .header("authorization", sign(signature))
                        .header("x-expected-method", method)
                        .body(AxumBody::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), expected);
            let body = to_bytes(response.into_body(), 4096).await.unwrap();
            if method == "HEAD" {
                assert!(body.is_empty());
            }
        }
    }
}
