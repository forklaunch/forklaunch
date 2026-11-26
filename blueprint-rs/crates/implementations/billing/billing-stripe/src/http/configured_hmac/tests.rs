use super::*;
use forklaunch_core::auth::jwt::{RemoteJwks, RemoteJwksConfig};
use forklaunch_core::http::RequestContext;
use jsonwebtoken::{Algorithm, Validation};
const KEY: &[u8] = b"owned-hmac-secret";
fn auth() -> HmacJwksAuth {
    HmacJwksAuth::new(
        KEY.to_vec(),
        RemoteJwks::new(RemoteJwksConfig::new("http://127.0.0.1:9/jwks")).unwrap(),
        Validation::new(Algorithm::RS256),
    )
    .unwrap()
}
fn route() -> RouteContract {
    forklaunch_billing_interface::module_contracts::module_registry(true)
        .routes()
        .iter()
        .find(|route| route.operation_name == "updatePaymentLink")
        .unwrap()
        .clone()
}
fn request(header: Option<String>) -> FrameworkRequest {
    let mut request = FrameworkRequest::new(
        route().method,
        "/payment-link/owned",
        RequestContext::new("hmac-test"),
    )
    .with_body(Body::Text("{\"amount\":\"12\"}".into()));
    if let Some(header) = header {
        request = request.with_header("authorization", header);
    }
    request
}
fn header(stamp: &str, canonical: &str) -> String {
    let payload = format!("PUT\n/owned\n{{\"amount\":\"12\"}}\n{canonical}\nrepeat");
    format!(
        "HMAC keyId=default ts={stamp} nonce=repeat signature={}",
        forklaunch_core::auth::hmac_sign(KEY, payload.as_bytes())
    )
}
#[test]
fn configured_header_errors_key_policy_and_clock_semantics() {
    let auth = auth();
    let route = route();
    let valid = header("2026-10-15T07:23:45.678Z", "2026-10-15T07:23:45.678Z");
    for (value, status) in [
        (None, 401),
        (Some(String::new()), 401),
        (Some("HMAC".into()), 401),
        (Some("HMAC keyId=default".into()), 403),
        (Some(valid.replace("HMAC", "OTHER")), 403),
        (Some(valid.replace("keyId=", "key=")), 401),
        (Some(valid.replace("keyId=default", "keyId=other")), 500),
        (
            Some(valid.replace("ts=2026-10-15T07:23:45.678Z", "ts=not-a-date")),
            500,
        ),
        (Some(valid.replace("signature=", "signature=bad")), 403),
    ] {
        assert_eq!(
            authenticate(&auth, &request(value), &route, true)
                .unwrap_err()
                .kind
                .status_code(),
            status
        );
    }
    for stamp in [
        "1900-01-01T00:00:00.000Z",
        "2100-01-01T00:00:00.000Z",
        "-271821-04-20T00:00:00.000Z",
        "+275760-09-13T00:00:00.000Z",
    ] {
        let value = header(stamp, stamp);
        assert!(authenticate(&auth, &request(Some(value.clone())), &route, true).is_ok());
        assert!(
            authenticate(
                &auth,
                &request(Some(format!("{value} ignored=extra"))),
                &route,
                true
            )
            .is_ok()
        );
    }
    for (stamp, canonical) in [
        ("2026-10-15", "2026-10-15T00:00:00.000Z"),
        ("2026-10-15T07:23:45.6Z", "2026-10-15T07:23:45.600Z"),
        ("2026-10-15T07:23:45.678999Z", "2026-10-15T07:23:45.678Z"),
        ("2026-10-15T07:23:45.678+02:00", "2026-10-15T05:23:45.678Z"),
    ] {
        assert!(
            authenticate(
                &auth,
                &request(Some(header(stamp, canonical))),
                &route,
                true
            )
            .is_ok(),
            "{stamp}"
        );
    }
    let private = auth
        .with_hmac_keys_for_route(&route, [("other".into(), b"different".to_vec())])
        .unwrap();
    let payload = b"PUT\n/owned\n{\"amount\":\"12\"}\n2026-10-15T07:23:45.678Z\nrepeat";
    let private_header = format!(
        "HMAC keyId=other ts=2026-10-15T07:23:45.678Z nonce=repeat signature={}",
        forklaunch_core::auth::hmac_sign(b"different", payload)
    );
    assert!(
        authenticate(
            &private,
            &request(Some(private_header.clone())),
            &route,
            true
        )
        .is_ok()
    );
    let mut other = route.clone();
    other.path = "/payment-link/another".into();
    assert_eq!(
        authenticate(&private, &request(Some(private_header)), &other, true)
            .unwrap_err()
            .kind,
        ErrorKind::Internal
    );
    assert_eq!(
        authenticate(&private, &request(Some(valid)), &route, true)
            .unwrap_err()
            .kind,
        ErrorKind::Internal
    );
}
#[test]
fn full_wire_path_is_canonical_and_router_relative_is_opt_in() {
    let auth = auth();
    let route = route();
    let stamp = "2026-10-15T07:23:45.678Z";
    let sign = |path: &str| {
        let payload = format!("PUT\n{path}\n{{\"amount\":\"12\"}}\n{stamp}\nfull");
        format!(
            "HMAC keyId=default ts={stamp} nonce=full signature={}",
            forklaunch_core::auth::hmac_sign(KEY, payload.as_bytes())
        )
    };
    for (path, default, opted_in) in [
        ("/payment-link/owned", true, true),
        // Deprecated router-relative form: rejected unless explicitly enabled.
        ("/owned", false, true),
        ("/plan/owned", false, false),
        ("/payment-link/other", false, false),
        ("/", false, false),
    ] {
        for (relative, accepted) in [(false, default), (true, opted_in)] {
            let result = authenticate(&auth, &request(Some(sign(path))), &route, relative);
            assert_eq!(result.is_ok(), accepted, "{path} relative={relative}");
            if !accepted {
                assert_eq!(result.unwrap_err().kind, ErrorKind::Forbidden);
            }
        }
    }
    // The SDK signer's header verifies unchanged.
    let header = forklaunch_core::auth::request_hmac::generate_hmac_auth_header(
        KEY,
        "default",
        "PUT",
        "/payment-link/owned",
        forklaunch_core::auth::request_hmac::SignedBody::Text("{\"amount\":\"12\"}"),
        stamp,
        "sdk",
    )
    .unwrap();
    assert!(authenticate(&auth, &request(Some(header)), &route, false).is_ok());
    assert_eq!(mount_relative("/payment-link", "/payment-link"), Some("/"));
    assert_eq!(mount_relative("/PLAN/x", "/plan"), Some("/x"));
    assert_eq!(mount_relative("/plan", "/payment-link"), None);
}
#[test]
fn full_time_clip_iso_and_selected_metadata() {
    for (value, expected) in [
        (0, "1970-01-01T00:00:00.000Z"),
        (-1, "1969-12-31T23:59:59.999Z"),
        (-8_640_000_000_000_000, "-271821-04-20T00:00:00.000Z"),
        (8_640_000_000_000_000, "+275760-09-13T00:00:00.000Z"),
    ] {
        assert_eq!(iso_timestamp(value), expected);
    }
    let registry = forklaunch_billing_interface::module_contracts::module_registry(true);
    let mut count = 0;
    for original in registry.routes() {
        let mut projected = original.clone();
        project_configured_billing_hmac(&mut projected);
        if selected(original) {
            count += 1;
            for status in [401, 403] {
                assert_eq!(
                    projected
                        .responses
                        .iter()
                        .find(|r| r.status_code == status)
                        .unwrap()
                        .body
                        .as_ref()
                        .unwrap()
                        .kind,
                    BodyKind::Text
                );
            }
        } else {
            assert_eq!(original, &projected);
        }
    }
    assert!(count > 10);
}
#[tokio::test]
async fn per_route_text_override_preserves_unselected_json_and_raw_path() {
    use forklaunch_axum::{AuthErrorFormat, ForkLaunchRouter, RouterConfig};
    use forklaunch_core::contract::HmacAuth;
    use std::sync::Arc;
    let selected_route = RouteContract::get("Selected", "Selected", "/payment-link/:id")
        .with_auth(AuthConfig::Hmac(HmacAuth::new()))
        .with_response(200, None);
    let other_route = RouteContract::get("Bound", "Bound", "/bound-payment-links/:id")
        .with_auth(AuthConfig::Hmac(HmacAuth::new()))
        .with_response(200, None);
    let verifier = Arc::new(auth());
    let config = RouterConfig::default()
        .with_optional_trailing_slash_for_path(&selected_route.openapi_path)
        .with_auth_error_format_for_route(&selected_route, AuthErrorFormat::TypeScriptText)
        .with_response_authenticator(move |request, route| {
            let verifier = Arc::clone(&verifier);
            async move {
                if selected(&route) {
                    authenticate(&verifier, &request, &route, true)
                } else {
                    verifier
                        .authenticate(request, route)
                        .await
                        .map_err(|e| e.to_error_response())
                }
            }
        });
    let app = ForkLaunchRouter::anonymous()
        .get(selected_route, || async { "ok" })
        .get(other_route, || async { "ok" })
        .with_config(config)
        .into_router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let http = reqwest::Client::new();
    for path in ["/payment-link/owned", "/bound-payment-links/owned"] {
        let response = http.get(format!("{url}{path}")).send().await.unwrap();
        assert_eq!(response.status(), 401);
        let kind = response.headers()["content-type"]
            .to_str()
            .unwrap()
            .to_owned();
        let text = response.text().await.unwrap();
        if path.starts_with("/payment-link/") {
            assert!(kind.starts_with("text/plain"));
            assert_eq!(text, "Authorization token required.");
        } else {
            assert!(kind.starts_with("application/json"));
        }
    }
    for (path, signed) in [
        ("/payment-link/owned?query=1", "/owned"),
        ("/payment-link/abc%2Fdef", "/abc%2Fdef"),
        ("/payment-link/owned/", "/owned/"),
        ("/payment-link/owned?query=1", "/payment-link/owned"),
        ("/payment-link/abc%2Fdef", "/payment-link/abc%2Fdef"),
        ("/payment-link/owned/", "/payment-link/owned/"),
    ] {
        let stamp = "1900-01-01T00:00:00.000Z";
        let payload = format!("GET\n{signed}\nundefined{stamp}\nnonce");
        let header = format!(
            "HMAC keyId=default ts={stamp} nonce=nonce signature={}",
            forklaunch_core::auth::hmac_sign(KEY, payload.as_bytes())
        );
        let response = http
            .get(format!("{url}{path}"))
            .header("authorization", header)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "{path}");
    }
    // The admitted alias still authenticates; generic/bound paths stay strict.
    let response = http
        .get(format!("{url}/payment-link/owned/"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    for path in ["/payment-link/owned//", "/bound-payment-links/owned/"] {
        assert_eq!(
            http.get(format!("{url}{path}"))
                .send()
                .await
                .unwrap()
                .status(),
            404
        );
    }
    task.abort();
    let _ = task.await;
}

#[test]
fn raw_path_preserves_signed_trailing_slash() {
    use forklaunch_core::contract::HttpMethod;
    // URI provenance survives normalization without changing the normalized route.
    let mut req = FrameworkRequest::new(
        HttpMethod::Get,
        "/payment-link/owned/",
        RequestContext::new("raw"),
    );
    req.raw_path = Some("/payment-link/owned/".into());
    let stamp = "1900-01-01T00:00:00.000Z";
    let payload = format!("GET\n/owned/\nundefined{stamp}\nnonce");
    req = req.with_header(
        "authorization",
        format!(
            "HMAC keyId=default ts={stamp} nonce=nonce signature={}",
            forklaunch_core::auth::hmac_sign(KEY, payload.as_bytes())
        ),
    );
    let mut route = route();
    route.method = HttpMethod::Get;
    assert!(authenticate(&auth(), &req, &route, true).is_ok());
    assert!(authenticate(&auth(), &req, &route, false).is_err());
}
