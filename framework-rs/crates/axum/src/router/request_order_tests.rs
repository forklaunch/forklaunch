use super::*;
use axum::{body::Body, http::Request};
use forklaunch_core::auth::Principal;
use forklaunch_core::contract::{BodyContract, ResponseContract};
use forklaunch_core::http::{BodyParserHooks, JsonRequestParsers};
use std::sync::{Mutex, atomic::AtomicUsize};

#[tokio::test]
async fn opt_in_order_preserves_defaults_modes_and_rejection_metadata() {
    for stage in [
        RequestValidationStage::BeforeAuthentication,
        RequestValidationStage::AfterAuthentication,
    ] {
        for mode in [
            ValidationMode::None,
            ValidationMode::Warning,
            ValidationMode::Error,
        ] {
            let mut route = RouteContract::get("Protected", "Protected", "/protected")
                .with_access(AccessLevel::Authenticated)
                .with_response(200, None);
            route.options.request_validation = mode;
            let calls = Arc::new(AtomicUsize::new(0));
            let validator_calls = calls.clone();
            let mut validators = RequestValidators::default();
            validators
                .insert_with_rejection(
                    &route,
                    ResponseContract::new(
                        400,
                        Some(BodyContract {
                            kind: BodyKind::Text,
                            schema: forklaunch_core::contract::SchemaRef::named("String"),
                            content_type: Some("text/plain".into()),
                        }),
                    ),
                    move |_, _| {
                        validator_calls.fetch_add(1, Ordering::SeqCst);
                        Some(FrameworkResponse::text(400, "public diagnostic"))
                    },
                )
                .unwrap();
            validators.set_stage(&route, stage).unwrap();
            let config = RouterConfig {
                request_validators: validators,
                ..RouterConfig::default()
            }
            .with_authenticator(|_, _| async {
                tokio::task::yield_now().await;
                Err(AuthError::MissingCredentials)
            });
            let app = ForkLaunchRouter::anonymous()
                .get(route, || async { "unexpected handler" })
                .with_config(config)
                .into_router();
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/protected")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let rejects = stage == RequestValidationStage::BeforeAuthentication
                && mode == ValidationMode::Error;
            assert_eq!(response.status(), if rejects { 400 } else { 401 });
            assert_eq!(calls.load(Ordering::SeqCst), usize::from(rejects));
        }
    }
}

#[tokio::test]
async fn parser_precedes_callback_and_success_calls_callback_once_before_async_auth() {
    let route = RouteContract::post("Parsed", "Parsed", "/parsed")
        .with_request_body(BodyContract::json("Input"))
        .with_access(AccessLevel::Authenticated)
        .with_response(200, None);
    let events = Arc::new(Mutex::new(Vec::new()));
    let callbacks = events.clone();
    let mut validators = RequestValidators::default();
    validators
        .insert(&route, move |request, raw_query| {
            assert_eq!(raw_query, Some("a=1&a=2"));
            assert_eq!(
                request.parsed_json.as_ref().unwrap().value.as_deref(),
                Some(&serde_json::json!({}))
            );
            assert!(request.session.is_none());
            callbacks.lock().unwrap().push("validate");
            None
        })
        .unwrap();
    validators
        .set_stage(&route, RequestValidationStage::BeforeAuthentication)
        .unwrap();
    let mut parsers = JsonRequestParsers::default();
    parsers.insert(&route, BodyParserHooks::default()).unwrap();
    let auth_events = events.clone();
    let config = RouterConfig {
        request_validators: validators,
        json_request_parsers: parsers,
        ..RouterConfig::default()
    }
    .with_authenticator(move |_, _| {
        let events = auth_events.clone();
        async move {
            tokio::task::yield_now().await;
            events.lock().unwrap().push("auth");
            Ok(AuthenticatedSession::new(
                Principal::new("subject"),
                serde_json::Value::Null,
            ))
        }
    });
    let handler_events = events.clone();
    let mut local = LocalRouter::new();
    local.register(route.clone(), move |_: FrameworkRequest| {
        handler_events.lock().unwrap().push("handler");
        Ok(FrameworkResponse::text(200, "ok"))
    });
    let app = ForkLaunchRouter::from_local_router(ServiceInfo::new("order", "1"), [route], local)
        .with_config(config)
        .into_router();
    for (body, status) in [("{", 400), ("{}", 200)] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/parsed?a=1&a=2")
                    .header("content-type", "application/json")
                    .header("content-length", body.len())
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        if status == 400 {
            assert!(events.lock().unwrap().is_empty());
        }
    }
    assert_eq!(*events.lock().unwrap(), ["validate", "auth", "handler"]);
}

#[test]
fn ordering_rejects_missing_and_stale_registration() {
    let route = RouteContract::get("Order", "Order", "/order");
    let mut validators = RequestValidators::default();
    assert!(
        validators
            .set_stage(&route, RequestValidationStage::BeforeAuthentication)
            .is_err()
    );
    validators.insert(&route, |_, _| None).unwrap();
    let changed = route.clone().with_request_body(BodyContract::json("New"));
    assert!(
        validators
            .set_stage(&changed, RequestValidationStage::BeforeAuthentication)
            .is_err()
    );
}

#[tokio::test]
async fn pre_auth_rejection_metadata_mismatch_is_redacted_before_authentication() {
    let route = RouteContract::get("Mismatch", "Mismatch", "/mismatch")
        .with_access(AccessLevel::Authenticated)
        .with_response(200, None);
    let mut validators = RequestValidators::default();
    validators
        .insert_with_rejection(
            &route,
            ResponseContract::new(
                400,
                Some(BodyContract {
                    kind: BodyKind::Text,
                    schema: forklaunch_core::contract::SchemaRef::named("String"),
                    content_type: Some("text/plain".into()),
                }),
            ),
            |_, _| Some(FrameworkResponse::text(401, "PRIVATE_CALLBACK")),
        )
        .unwrap();
    validators
        .set_stage(&route, RequestValidationStage::BeforeAuthentication)
        .unwrap();
    let auth_calls = Arc::new(AtomicUsize::new(0));
    let calls = auth_calls.clone();
    let config = RouterConfig {
        request_validators: validators,
        ..RouterConfig::default()
    }
    .with_authenticator(move |_, _| {
        calls.fetch_add(1, Ordering::SeqCst);
        async { Err(AuthError::MissingCredentials) }
    });
    let app = ForkLaunchRouter::anonymous()
        .get(route, || async { "unexpected handler" })
        .with_config(config)
        .into_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/mismatch")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 500);
    assert_eq!(auth_calls.load(Ordering::SeqCst), 0);
    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("PRIVATE_CALLBACK"));
}
