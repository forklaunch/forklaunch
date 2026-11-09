use super::*;
use forklaunch_core::auth::{jwt::RemoteJwksConfig, request_hmac::generate_hmac_auth_header};
use forklaunch_core::contract::{HmacAuth, HttpMethod};
use forklaunch_core::http::RequestContext;

fn route(method: HttpMethod, path: &str) -> RouteContract {
    RouteContract::new("Relay", "Relay", method, path).with_auth(AuthConfig::Hmac(HmacAuth::new()))
}
fn base() -> HmacJwksAuth {
    HmacJwksAuth::new(
        b"default-secret".to_vec(),
        RemoteJwks::new(RemoteJwksConfig::new("http://127.0.0.1:9/jwks")).unwrap(),
        Validation::new(Algorithm::RS256),
    )
    .unwrap()
}
#[tokio::test]
async fn per_route_keys_do_not_authorize_other_methods_paths_or_default_keys() {
    let ingest = route(HttpMethod::Post, "/relay/session-ingest");
    let auth = base()
        .with_hmac_keys_for_route(
            &ingest,
            [("instance-1".into(), b"instance-secret".to_vec())],
        )
        .unwrap();
    let now = chrono::DateTime::<chrono::Utc>::from(SystemTime::now())
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    for (method, path, id, secret, allowed) in [
        (
            HttpMethod::Post,
            "/relay/session-ingest",
            "instance-1",
            "instance-secret",
            true,
        ),
        (
            HttpMethod::Post,
            "/relay/session-ingest",
            "default",
            "default-secret",
            false,
        ),
        (
            HttpMethod::Post,
            "/relay/session-ingest",
            "instance-1",
            "default-secret",
            false,
        ),
        (
            HttpMethod::Post,
            "/other",
            "instance-1",
            "instance-secret",
            false,
        ),
        (
            HttpMethod::Get,
            "/relay/session-ingest",
            "instance-1",
            "instance-secret",
            false,
        ),
        (
            HttpMethod::Post,
            "/other",
            "default",
            "default-secret",
            true,
        ),
    ] {
        let signature = generate_hmac_auth_header(
            secret.as_bytes(),
            id,
            method.as_str(),
            path,
            SignedBody::Text("{}"),
            &now,
            "nonce",
        )
        .unwrap();
        let request = FrameworkRequest::new(method, path, RequestContext::new("route-key-fixture"))
            .with_header("authorization", signature)
            .with_body(Body::Text("{}".into()));
        let result = auth.authenticate(request, route(method, path)).await;
        assert_eq!(result.is_ok(), allowed, "{method:?} {path} {id}");
    }
}
#[test]
fn invalid_route_key_policies_are_rejected() {
    let route = route(HttpMethod::Post, "/relay");
    for keys in [
        vec![],
        vec![(String::new(), vec![1])],
        vec![("instance".into(), vec![])],
        vec![("instance".into(), vec![1]), ("instance".into(), vec![2])],
    ] {
        assert!(base().with_hmac_keys_for_route(&route, keys).is_err());
    }
    assert!(
        base()
            .with_hmac_keys_for_route(
                &RouteContract::get("Read", "Read", "/read"),
                [("instance".into(), vec![1])]
            )
            .is_err()
    );
}
