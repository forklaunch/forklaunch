use super::*;
use crate::{ParsedQuery, RequestFieldValues, RequestTransformOutput};
use axum::{body::Body, http::Request};
use forklaunch_core::contract::{
    BodyContract, FieldContract, RbacConfig, ResponseContract, SchemaRef,
};
use forklaunch_core::http::{BodyParserHooks, JsonRequestParsers};
use serde_json::{Value, json};
use std::sync::atomic::AtomicUsize;
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
#[tokio::test]
async fn accepted_request_schema_never_skips_access_authentication_or_rbac() {
    for mode in [
        ValidationMode::None,
        ValidationMode::Warning,
        ValidationMode::Error,
    ] {
        for (authenticated, admin, status) in
            [(false, false, 401), (true, false, 403), (true, true, 200)]
        {
            let mut route = RouteContract::get("Fields", "Fields", "/fields")
                .with_access(AccessLevel::Protected)
                .with_rbac(RbacConfig {
                    allowed_roles: vec!["admin".into()],
                    ..RbacConfig::default()
                })
                .with_query(FieldContract::required("limit", "Number"))
                .with_header(FieldContract::required("x-default", "String"))
                .with_response(200, None);
            route.options.request_validation = mode;
            let mut validators = RequestValidators::default();
            validators
                .insert_request_transform(
                    &route,
                    &JsonRequestParsers::default(),
                    rejection(),
                    |_, _| {
                        Ok(RequestTransformOutput {
                            fields: RequestFieldValues {
                                query: [("limit".into(), json!(7))].into(),
                                headers: [
                                    ("authorization".into(), json!(2)),
                                    ("x-default".into(), json!("default")),
                                ]
                                .into(),
                                ..RequestFieldValues::default()
                            },
                            body: None,
                        })
                    },
                )
                .unwrap();
            let calls = Arc::new(AtomicUsize::new(0));
            let auth_calls = calls.clone();
            let config = RouterConfig {
                request_validators: validators,
                ..RouterConfig::default()
            }
            .with_authenticator(move |request, _| {
                auth_calls.fetch_add(1, Ordering::SeqCst);
                assert_eq!(request.headers["authorization"], "Bearer original");
                let fields = request.parsed_fields.unwrap();
                assert_eq!(fields.values().headers["authorization"], json!(2));
                assert!(!fields.values().headers.contains_key("x-default"));
                assert!(request.version.is_none());
                async move {
                    if !authenticated {
                        return Err(AuthError::MissingCredentials);
                    }
                    let mut principal = forklaunch_core::auth::Principal::new("actor");
                    if admin {
                        principal.roles.push("admin".into());
                    }
                    Ok(AuthenticatedSession::new(principal, Value::Null))
                }
            });
            let app =
                ForkLaunchRouter::anonymous()
                    .get(
                        route,
                        |ParsedQuery(query): ParsedQuery<
                            std::collections::BTreeMap<String, u64>,
                        >| async move {
                            assert_eq!(query["limit"], 7);
                            "ok"
                        },
                    )
                    .with_config(config)
                    .into_router();
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/fields")
                        .header("authorization", "Bearer original")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), status);
            assert_eq!(calls.load(Ordering::SeqCst), 1);
        }
    }
}
#[test]
fn mixed_failure_rolls_back_body_and_every_field_for_all_modes() {
    for mode in [
        ValidationMode::None,
        ValidationMode::Warning,
        ValidationMode::Error,
    ] {
        let mut route = RouteContract::post("Mixed", "Mixed", "/mixed")
            .with_request_body(BodyContract::json("Input"));
        route.options.request_validation = mode;
        let mut parsers = JsonRequestParsers::default();
        parsers.insert(&route, BodyParserHooks::default()).unwrap();
        let mut validators = RequestValidators::default();
        validators
            .insert_request_transform(&route, &parsers, rejection(), |request, raw| {
                assert_eq!(raw, Some("limit=1&limit=2"));
                assert!(request.parsed_json.is_some());
                Err(FrameworkResponse::text(400, "public failure"))
            })
            .unwrap();
        let mut request =
            FrameworkRequest::new(HttpMethod::Post, "/mixed", RequestContext::new("mixed"))
                .with_header("content-type", "application/json")
                .with_header("content-length", "14")
                .with_body(FrameworkBody::Text("{\"amount\":\"2\"}".into()));
        request.raw_query = Some("limit=1&limit=2".into());
        request.parsed_json = parsers.parse(&route, &request).unwrap();
        let before = request.clone();
        let response = validators.transform_request(&route, &mut request, Some("limit=1&limit=2"));
        assert_eq!(response.is_some(), mode == ValidationMode::Error);
        assert_eq!(request, before);
        assert!(request.parsed_fields.is_none());
        assert!(!request.parsed_json.unwrap().is_transformed());
    }
}
#[tokio::test]
async fn mixed_local_projection_is_once_and_retains_wire_query_headers_body() {
    let route = RouteContract::post("Mixed", "Mixed", "/mixed/:id")
        .with_request_body(BodyContract::json("Input"))
        .with_response(200, None);
    let mut parsers = JsonRequestParsers::default();
    parsers.insert(&route, BodyParserHooks::default()).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    let mut validators = RequestValidators::default();
    validators
        .insert_request_transform(&route, &parsers, rejection(), move |request, raw| {
            counted.fetch_add(1, Ordering::SeqCst);
            assert_eq!(request.params["id"], "2");
            assert_eq!(raw, Some("limit=1&limit=2"));
            Ok(RequestTransformOutput {
                fields: RequestFieldValues {
                    params: [("id".into(), json!(2))].into(),
                    query: [("limit".into(), json!([1, 2]))].into(),
                    headers: [
                        ("x-count".into(), json!(0)),
                        ("x-new".into(), json!("default")),
                    ]
                    .into(),
                },
                body: Some(json!({"amount":2})),
            })
        })
        .unwrap();
    let mut local = LocalRouter::new();
    local.register(route.clone(), |request: FrameworkRequest| {
        assert_eq!(request.raw_query.as_deref(), Some("limit=1&limit=2"));
        assert_eq!(request.params["id"], "2");
        assert_eq!(request.headers["x-count"], "0");
        assert_eq!(request.body.as_text(), Some("{\"amount\":\"2\"}"));
        let fields = request.parsed_fields.unwrap();
        assert_eq!(fields.values().params["id"], json!(2));
        assert_eq!(fields.values().query["limit"], json!([1, 2]));
        assert_eq!(fields.values().headers["x-count"], json!("0"));
        assert!(!fields.values().headers.contains_key("x-new"));
        assert_eq!(
            request.parsed_json.unwrap().value.as_deref(),
            Some(&json!({"amount":2}))
        );
        Ok(FrameworkResponse::text(200, "ok"))
    });
    let app = ForkLaunchRouter::from_local_router(ServiceInfo::new("fields", "1"), [route], local)
        .with_config(RouterConfig {
            request_validators: validators,
            json_request_parsers: parsers,
            ..RouterConfig::default()
        })
        .into_router();
    let body = "{\"amount\":\"2\"}";
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mixed/2?limit=1&limit=2")
                .header("content-type", "application/json")
                .header("content-length", body.len())
                .header("x-count", "0")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
#[test]
fn whole_request_registration_requires_json_for_bodies_and_cannot_move_after_auth() {
    let route = RouteContract::get("Fields", "Fields", "/fields");
    let mut validators = RequestValidators::default();
    validators
        .insert_request_transform(
            &route,
            &JsonRequestParsers::default(),
            rejection(),
            |_, _| Ok(RequestTransformOutput::default()),
        )
        .unwrap();
    assert!(
        validators
            .set_stage(&route, RequestValidationStage::AfterAuthentication)
            .is_err()
    );
    let body_route = route.with_request_body(BodyContract::json("Input"));
    let mut other = RequestValidators::default();
    assert!(
        other
            .insert_request_transform(
                &body_route,
                &JsonRequestParsers::default(),
                rejection(),
                |_, _| Ok(RequestTransformOutput::default())
            )
            .is_err()
    );
}

#[test]
fn strict_zod_string_transform_commits_all_sections_atomically() {
    use forklaunch_validator::typescript_coerce::{ZodStringStep, zod_string};
    for mode in [
        ValidationMode::None,
        ValidationMode::Warning,
        ValidationMode::Error,
    ] {
        for (label, succeeds) in [("  ab  ", true), (" a ", false)] {
            let mut route = RouteContract::post("Strings", "Strings", "/strings")
                .with_request_body(BodyContract::json("Input"));
            route.options.request_validation = mode;
            let mut parsers = JsonRequestParsers::default();
            parsers.insert(&route, BodyParserHooks::default()).unwrap();
            let mut validators = RequestValidators::default();
            validators
                .insert_request_transform(&route, &parsers, rejection(), |request, _| {
                    let fields = RequestFieldValues {
                        query: [("limit".into(), json!(7))].into(),
                        ..RequestFieldValues::default()
                    };
                    let input = request
                        .parsed_json
                        .as_ref()
                        .unwrap()
                        .value
                        .as_deref()
                        .unwrap();
                    let label = zod_string(
                        input.get("label"),
                        &[ZodStringStep::Trim, ZodStringStep::Min(2)],
                    )
                    .map_err(|errors| FrameworkResponse::text(400, errors[0].message.clone()))?;
                    Ok(RequestTransformOutput {
                        fields,
                        body: Some(json!({"label": label})),
                    })
                })
                .unwrap();
            let wire = json!({"label":label,"extra":"preserved on failure"}).to_string();
            let mut request =
                FrameworkRequest::new(HttpMethod::Post, "/strings", RequestContext::new("strings"))
                    .with_header("content-type", "application/json")
                    .with_header("content-length", wire.len().to_string())
                    .with_body(FrameworkBody::Text(wire.clone()));
            request.raw_query = Some("limit=raw".into());
            request.parsed_json = parsers.parse(&route, &request).unwrap();
            let before = request.clone();
            let response = validators.transform_request(&route, &mut request, Some("limit=raw"));
            assert_eq!(
                response.is_some(),
                !succeeds && mode == ValidationMode::Error
            );
            assert_eq!(request.body.as_text(), Some(wire.as_str()));
            assert_eq!(request.raw_query, before.raw_query);
            if succeeds {
                assert_eq!(
                    request.parsed_fields.as_ref().unwrap().values().query["limit"],
                    json!(7)
                );
                assert_eq!(
                    request.parsed_json.as_ref().unwrap().value.as_deref(),
                    Some(&json!({"label":"ab"}))
                );
                assert!(request.parsed_json.as_ref().unwrap().is_transformed());
            } else {
                assert_eq!(request, before);
                assert!(request.parsed_fields.is_none());
                assert!(!request.parsed_json.as_ref().unwrap().is_transformed());
            }
        }
    }
}

#[test]
fn ordered_zod_number_callback_never_commits_partial_fields() {
    use forklaunch_validator::typescript_coerce::{ZodNumberStep, zod_number};
    for mode in [
        ValidationMode::None,
        ValidationMode::Warning,
        ValidationMode::Error,
    ] {
        for (input, accepted) in [("12.99", true), ("Infinity", false)] {
            let mut route = RouteContract::get("Number", "Number", "/number");
            route.options.request_validation = mode;
            let mut validators = RequestValidators::default();
            validators
                .insert_request_transform(
                    &route,
                    &JsonRequestParsers::default(),
                    rejection(),
                    |request, _| {
                        let mut fields = RequestFieldValues::default();
                        fields.params.insert("default".into(), json!(7));
                        let input = json!(request.query["amount"]);
                        let amount =
                            zod_number(Some(&input), &[ZodNumberStep::Finite, ZodNumberStep::Safe])
                                .map_err(|errors| {
                                    FrameworkResponse::text(400, errors[0].message.clone())
                                })?;
                        fields.query.insert("amount".into(), json!(amount));
                        Ok(RequestTransformOutput { fields, body: None })
                    },
                )
                .unwrap();
            let mut request =
                FrameworkRequest::new(HttpMethod::Get, "/number", RequestContext::new("number"));
            request.query.insert("amount".into(), input.into());
            let before = request.clone();
            let response = validators.transform_request(&route, &mut request, None);
            assert_eq!(
                response.is_some(),
                !accepted && mode == ValidationMode::Error
            );
            if accepted {
                let fields = request.parsed_fields.unwrap();
                assert_eq!(fields.values().params["default"], json!(7));
                assert_eq!(fields.values().query["amount"], json!(12.99));
                assert_eq!(request.query, before.query);
                assert_eq!(request.body, before.body);
            } else {
                assert_eq!(request, before);
                assert!(request.parsed_fields.is_none());
            }
        }
    }
}
