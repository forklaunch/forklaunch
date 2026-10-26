use super::*;
use axum::{Extension, body::Body};
use forklaunch_core::contract::BodyContract;
use forklaunch_core::http::{BodyParserHooks, JsonRequestParsers, ParsedJsonBody};
use std::sync::atomic::AtomicUsize;

fn route() -> RouteContract {
    let mut route = RouteContract::post("Parse", "Parse", "/parse")
        .with_request_body(BodyContract::json("Input"))
        .with_response(200, None);
    route.options.request_validation = ValidationMode::None;
    route
}
fn request(body: &str) -> Request {
    Request::builder()
        .method("POST")
        .uri("/parse")
        .header("content-type", "application/json")
        .header("content-length", body.len())
        .body(Body::from(body.to_owned()))
        .unwrap()
}
#[tokio::test]
async fn parser_is_opt_in_and_fatal_before_auth_in_every_schema_mode() {
    for mode in [
        ValidationMode::None,
        ValidationMode::Warning,
        ValidationMode::Error,
    ] {
        let mut route = route().with_access(AccessLevel::Authenticated);
        route.options.request_validation = mode;
        let auth_calls = Arc::new(AtomicUsize::new(0));
        let calls = auth_calls.clone();
        let mut parsers = JsonRequestParsers::default();
        parsers.insert(&route, BodyParserHooks::default()).unwrap();
        let config = RouterConfig {
            json_request_parsers: parsers,
            ..RouterConfig::default()
        }
        .with_authenticator(move |request, _| {
            assert_eq!(
                request.parsed_json.unwrap().value.as_deref(),
                Some(&serde_json::json!({}))
            );
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err(AuthError::MissingCredentials) }
        });
        let app = ForkLaunchRouter::anonymous()
            .post(route.clone(), || async { "ok" })
            .with_config(config)
            .into_router();
        let response = app.clone().oneshot(request("{")).await.unwrap();
        assert_eq!(response.status(), 400);
        assert_eq!(auth_calls.load(Ordering::SeqCst), 0);
        assert_eq!(app.oneshot(request("{}")).await.unwrap().status(), 401);
        assert_eq!(auth_calls.load(Ordering::SeqCst), 1);
        let native = ForkLaunchRouter::anonymous()
            .post(route, || async { "ok" })
            .into_router();
        assert_eq!(native.oneshot(request("{")).await.unwrap().status(), 401);
    }
}
#[tokio::test]
async fn local_adapter_verifies_once_and_preserves_original_body() {
    let mut route = route();
    route.options.request_validation = ValidationMode::Error;
    let verify_calls = Arc::new(AtomicUsize::new(0));
    let calls = verify_calls.clone();
    let mut parsers = JsonRequestParsers::default();
    parsers
        .insert(
            &route,
            BodyParserHooks::default().with_verify(move |_, _, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
        )
        .unwrap();
    let mut handlers = LocalRouter::new();
    handlers.register(route.clone(), |request: FrameworkRequest| {
        assert!(request.version.is_none());
        assert_eq!(request.body.as_text(), Some("{\"amount\":12.99}"));
        assert_eq!(request.parsed_json.unwrap().value.unwrap()["amount"], 12.99);
        Ok(FrameworkResponse::text(200, "ok"))
    });
    let app =
        ForkLaunchRouter::from_local_router(ServiceInfo::new("parser", "1"), [route], handlers)
            .with_config(RouterConfig {
                json_request_parsers: parsers,
                ..RouterConfig::default()
            })
            .into_router();
    assert_eq!(
        app.oneshot(request("{\"amount\":12.99}"))
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(verify_calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn framed_empty_json_is_present_for_native_required_body_checks() {
    let mut route = route();
    route.options.request_validation = ValidationMode::Error;
    let mut parsers = JsonRequestParsers::default();
    parsers.insert(&route, BodyParserHooks::default()).unwrap();
    let app = ForkLaunchRouter::anonymous()
        .post(
            route,
            |Extension(parsed): Extension<ParsedJsonBody>| async move {
                assert_eq!(parsed.value.as_deref(), Some(&serde_json::json!({})));
                "ok"
            },
        )
        .with_config(RouterConfig {
            json_request_parsers: parsers,
            ..RouterConfig::default()
        })
        .into_router();
    assert_eq!(app.oneshot(request("")).await.unwrap().status(), 200);
}
