use super::*;
use axum::{Extension, body::Body, http::Request};
use forklaunch_core::contract::{BodyContract, ResponseContract, SchemaRef};
use forklaunch_core::http::{BodyParserHooks, JsonRequestParsers, ParsedJsonBody};
use serde_json::json;
use std::sync::atomic::AtomicUsize;
fn route() -> RouteContract {
    RouteContract::post("Transform", "Transform", "/transform")
        .with_request_body(BodyContract::json("Input"))
        .with_response(200, None)
}
fn rejection() -> ResponseContract {
    ResponseContract::new(
        400,
        Some(BodyContract {
            kind: BodyKind::Text,
            schema: SchemaRef::named("String"),
            content_type: Some("text/plain".into()),
        }),
    )
}
fn parsers(route: &RouteContract) -> JsonRequestParsers {
    let mut parsers = JsonRequestParsers::default();
    parsers.insert(route, BodyParserHooks::default()).unwrap();
    parsers
}
#[test]
fn transform_registration_requires_matching_parser_and_fixed_pre_auth_stage() {
    let route = route();
    let parsers = parsers(&route);
    let mut validators = RequestValidators::default();
    assert!(
        validators
            .insert_json_transform(&route, &JsonRequestParsers::default(), rejection(), |_| Ok(
                None
            ))
            .is_err()
    );
    validators
        .insert_json_transform(&route, &parsers, rejection(), |_| Ok(None))
        .unwrap();
    assert!(
        validators
            .set_stage(&route, RequestValidationStage::AfterAuthentication)
            .is_err()
    );
    assert!(
        validators
            .validate_json_parsers(&[route], &JsonRequestParsers::default())
            .is_err()
    );
}
#[test]
fn failed_transforms_preserve_entire_value_bytes_and_unset_marker_in_all_modes() {
    for mode in [
        ValidationMode::None,
        ValidationMode::Warning,
        ValidationMode::Error,
    ] {
        let mut route = route();
        route.options.request_validation = mode;
        let parsers = parsers(&route);
        let mut validators = RequestValidators::default();
        validators
            .insert_json_transform(&route, &parsers, rejection(), |request| {
                let mut candidate = request
                    .parsed_json
                    .as_ref()
                    .unwrap()
                    .value
                    .as_deref()
                    .unwrap()
                    .clone();
                candidate["amount"] = json!(2);
                Err(FrameworkResponse::text(400, "public failure"))
            })
            .unwrap();
        let mut request =
            FrameworkRequest::new(HttpMethod::Post, "/transform", RequestContext::new("test"))
                .with_header("content-type", "application/json")
                .with_header("content-length", "27")
                .with_body(FrameworkBody::Text(
                    "{\"amount\":\"bad\",\"extra\":true}".into(),
                ));
        request.parsed_json = parsers.parse(&route, &request).unwrap();
        let before = request.clone();
        let result = validators.transform_request(&route, &mut request, None);
        assert_eq!(result.is_some(), mode == ValidationMode::Error);
        assert_eq!(request, before);
        assert!(!request.parsed_json.unwrap().is_transformed());
    }
}
#[tokio::test]
async fn successful_transforms_reach_auth_and_each_adapter_once_with_original_bytes() {
    for local in [false, true] {
        let route = route().with_access(AccessLevel::Authenticated);
        let parsers = parsers(&route);
        let mut validators = RequestValidators::default();
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        validators
            .insert_json_transform(&route, &parsers, rejection(), move |_| {
                counted.fetch_add(1, Ordering::SeqCst);
                Ok(Some(json!({"amount":2})))
            })
            .unwrap();
        let config = RouterConfig {
            request_validators: validators,
            json_request_parsers: parsers,
            ..RouterConfig::default()
        }
        .with_authenticator(|request, _| async move {
            assert_eq!(
                request.parsed_json.unwrap().value.as_deref(),
                Some(&json!({"amount":2}))
            );
            Ok(AuthenticatedSession::new(
                forklaunch_core::auth::Principal::new("actor"),
                serde_json::Value::Null,
            ))
        });
        let app = if local {
            let mut handlers = LocalRouter::new();
            handlers.register(route.clone(), |request: FrameworkRequest| {
                assert_eq!(
                    request.body.as_text(),
                    Some("{\"amount\":\"2\",\"extra\":true}")
                );
                assert_eq!(
                    request.parsed_json.unwrap().value.as_deref(),
                    Some(&json!({"amount":2}))
                );
                Ok(FrameworkResponse::text(200, "ok"))
            });
            ForkLaunchRouter::from_local_router(
                ServiceInfo::new("transform", "1"),
                [route],
                handlers,
            )
        } else {
            ForkLaunchRouter::anonymous().post(
                route,
                |Extension(parsed): Extension<ParsedJsonBody>, body: bytes::Bytes| async move {
                    assert_eq!(parsed.value.as_deref(), Some(&json!({"amount":2})));
                    assert!(parsed.is_transformed());
                    assert_eq!(
                        parsed.raw_body.as_deref().unwrap().as_slice(),
                        body.as_ref()
                    );
                    "ok"
                },
            )
        }
        .with_config(config)
        .into_router();
        let body = "{\"amount\":\"2\",\"extra\":true}";
        assert_eq!(
            app.oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/transform")
                    .header("content-type", "application/json")
                    .header("content-length", body.len())
                    .body(Body::from(body))
                    .unwrap()
            )
            .await
            .unwrap()
            .status(),
            200
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
#[tokio::test]
async fn accepted_absent_defaults_and_root_removal_skip_only_body_checks() {
    for output in [None, Some(json!({"amount":7}))] {
        for require_header in [false, true] {
            let mut route = route();
            if require_header {
                route = route.with_header(forklaunch_core::contract::FieldContract::required(
                    "x-required",
                    "String",
                ));
            }
            let parsers = parsers(&route);
            let mut validators = RequestValidators::default();
            let result = output.clone();
            validators
                .insert_json_transform(&route, &parsers, rejection(), move |request| {
                    assert!(request.parsed_json.as_ref().unwrap().value.is_none());
                    Ok(result.clone())
                })
                .unwrap();
            let expected = output.clone();
            let app = ForkLaunchRouter::anonymous()
                .post(
                    route,
                    move |Extension(parsed): Extension<ParsedJsonBody>| async move {
                        assert_eq!(parsed.value.as_deref(), expected.as_ref());
                        assert!(parsed.raw_body.is_none());
                        "ok"
                    },
                )
                .with_config(RouterConfig {
                    request_validators: validators,
                    json_request_parsers: parsers,
                    ..RouterConfig::default()
                })
                .into_router();
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/transform")
                        .header("content-type", "text/plain")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), if require_header { 400 } else { 200 });
        }
    }
}

#[test]
fn operational_transform_failures_are_never_validation_mode_suppressed() {
    for mode in [
        ValidationMode::None,
        ValidationMode::Warning,
        ValidationMode::Error,
    ] {
        let mut route = route();
        route.options.request_validation = mode;
        let parsers = parsers(&route);
        let mut validators = RequestValidators::default();
        validators
            .insert_json_transform_checked(&route, &parsers, rejection(), |_| {
                Err(JsonTransformFailure::Operational)
            })
            .unwrap();
        let mut request = FrameworkRequest::new(
            HttpMethod::Post,
            "/transform",
            RequestContext::new("safe-correlation"),
        )
        .with_header("content-type", "application/json")
        .with_header("content-length", "17")
        .with_body(FrameworkBody::Text("{\"private\":\"yes\"}".into()));
        request.parsed_json = parsers.parse(&route, &request).unwrap();
        let before = request.parsed_json.as_ref().unwrap().value.clone();
        let response = validators
            .transform_request(&route, &mut request, None)
            .unwrap();
        assert_eq!(response.status_code, 500);
        assert_eq!(request.parsed_json.as_ref().unwrap().value, before);
        let body = format!("{:?}", response.body);
        assert!(body.contains("safe-correlation"));
        assert!(!body.contains("private"));
        assert!(!body.contains("yes"));
    }
}
