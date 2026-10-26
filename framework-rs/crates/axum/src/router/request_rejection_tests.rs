use super::*;
use crate::{ForkLaunchRouter, RouterConfig};
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use forklaunch_core::ServiceInfo;
use forklaunch_core::contract::{BodyContract, BodyKind, FieldContract, HttpMethod, SchemaRef};
use forklaunch_core::http::RequestContext;
use std::sync::atomic::{AtomicUsize, Ordering};
use tower::ServiceExt;

fn route() -> RouteContract {
    RouteContract::get("Query", "Query", "/query")
        .with_query(FieldContract::optional("limit", "Integer"))
        .with_response(200, None)
}
fn declaration() -> ResponseContract {
    ResponseContract::new(
        400,
        Some(BodyContract {
            kind: BodyKind::Text,
            schema: SchemaRef::named("String"),
            content_type: Some("text/plain".into()),
        }),
    )
}
fn validators() -> RequestValidators {
    let mut validators = RequestValidators::default();
    validators
        .insert_with_rejection(&route(), declaration(), |_, _| {
            Some(FrameworkResponse::text(400, "public diagnostic"))
        })
        .unwrap();
    validators
}

#[test]
fn declarations_fail_closed_on_bad_metadata_and_conflicts() {
    let mut validators = RequestValidators::default();
    for status in [200, 399, 600] {
        assert!(
            validators
                .insert_with_rejection(&route(), ResponseContract::new(status, None), |_, _| None)
                .is_err()
        );
    }
    for media in [
        None,
        Some(""),
        Some("not-a-media-type"),
        Some("text/plain\r\nprivate:yes"),
    ] {
        let mut rejection = declaration();
        rejection.body.as_mut().unwrap().content_type = media.map(str::to_owned);
        assert!(
            validators
                .insert_with_rejection(&route(), rejection, |_, _| None)
                .is_err()
        );
    }
    let conflict = route().with_response(400, Some(BodyContract::json("Different")));
    assert!(
        validators
            .insert_with_rejection(&conflict, declaration(), |_, _| None)
            .is_err()
    );
    let declared = route().with_response(400, declaration().body);
    validators
        .insert_with_rejection(&declared, declaration(), |_, _| None)
        .unwrap();
    let mut registry = ContractRegistry::new();
    registry.register(conflict);
    assert!(validators.project(&registry).is_err());
}

#[test]
fn callback_status_and_media_are_enforced_without_leaking_details() {
    let request = FrameworkRequest::new(
        HttpMethod::Get,
        "/query",
        RequestContext::new("rejection-test"),
    );
    for response in [
        FrameworkResponse::text(403, "PRIVATE"),
        FrameworkResponse::json(400, "PRIVATE"),
        FrameworkResponse::new(400),
    ] {
        let mut validators = RequestValidators::default();
        validators
            .insert_with_rejection(&route(), declaration(), move |_, _| Some(response.clone()))
            .unwrap();
        let result = validators.reject(&route(), &request, None).unwrap();
        assert_eq!(result.status_code, 500);
        assert!(!result.body.as_text().unwrap().contains("PRIVATE"));
        assert!(result.body.as_text().unwrap().contains("rejection-test"));
    }
    let result = validators().reject(&route(), &request, None).unwrap();
    assert_eq!(result.status_code, 400);
}

#[tokio::test]
async fn rejection_metadata_survives_nesting_in_registry_artifact_and_openapi() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let child = ForkLaunchRouter::new(ServiceInfo::new("child", "1"))
        .with_config(RouterConfig {
            request_validators: validators(),
            ..RouterConfig::default()
        })
        .get(route(), move || {
            let count = count.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                "handler"
            }
        });
    let router = ForkLaunchRouter::new(ServiceInfo::new("parent", "1")).nest("/nested", child);
    let exported = router.registry().clone().into_artifact();
    assert_eq!(
        exported.routes[0]
            .responses
            .iter()
            .filter(|r| r.status_code == 400)
            .count(),
        1
    );
    assert!(exported.to_json().contains("text/plain"));
    let app = router.into_router();
    let document = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_slice(&to_bytes(document.into_body(), 65536).await.unwrap()).unwrap();
    assert!(
        json["paths"]["/nested/query"]["get"]["responses"]["400"]["content"]["text/plain"]
            .is_object()
    );
    let response = app
        .oneshot(
            Request::builder()
                .uri("/nested/query?limit=bad")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn changing_config_removes_only_projected_metadata() {
    let router = ForkLaunchRouter::new(ServiceInfo::new("config", "1"))
        .get(route(), || async { "ok" })
        .with_config(RouterConfig {
            request_validators: validators(),
            ..RouterConfig::default()
        });
    assert!(
        router.registry().routes()[0]
            .responses
            .iter()
            .any(|r| r.status_code == 400)
    );
    let router = router.with_config(RouterConfig::default());
    assert!(
        !router.registry().routes()[0]
            .responses
            .iter()
            .any(|r| r.status_code == 400)
    );
    for mode in [ValidationMode::Warning, ValidationMode::None] {
        let mut contract = route();
        contract.options.request_validation = mode;
        let mut registry = ContractRegistry::new();
        registry.register(contract);
        let projected = validators().project(&registry).unwrap().unwrap();
        assert!(
            !projected.routes()[0]
                .responses
                .iter()
                .any(|r| r.status_code == 400)
        );
    }
}

#[test]
fn empty_text_and_valid_media_tokens_are_accepted() {
    let request = FrameworkRequest::new(
        HttpMethod::Get,
        "/query",
        RequestContext::new("empty-rejection"),
    );
    let mut registered = RequestValidators::default();
    registered
        .insert_with_rejection(&route(), declaration(), |_, _| {
            Some(FrameworkResponse::text(400, ""))
        })
        .unwrap();
    let response = registered.reject(&route(), &request, None).unwrap();
    assert_eq!(response.status_code, 400);
    assert_eq!(response.body.as_text(), Some(""));
    for token in [
        "application/vnd.example%value",
        "application/x'*`|~test",
        "text/plain;charset=utf-8",
    ] {
        assert!(valid_media_type(token));
    }
    for token in ["text/pla in", "text/plain/extra", "text/(plain)"] {
        assert!(!valid_media_type(token));
    }
}
