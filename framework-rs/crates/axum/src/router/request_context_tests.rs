use super::*;
use crate::RequestContext;
use axum::{
    Extension, Json,
    body::{Body, to_bytes},
    http::Request,
};
use forklaunch_core::auth::Principal;
use serde_json::{Value, json};
use std::sync::Mutex;
use tower::ServiceExt;

#[tokio::test]
async fn handlers_receive_exact_authenticated_context_for_explicit_and_generated_ids() {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let auth_contexts = captured.clone();
    let config = RouterConfig::default().with_authenticator(move |request, _| {
        auth_contexts.lock().unwrap().push(request.context);
        async {
            Ok(AuthenticatedSession::new(
                Principal::new("verified-actor"),
                Value::Null,
            ))
        }
    });
    let route = RouteContract::get("Context", "Context", "/context")
        .with_access(AccessLevel::Authenticated)
        .with_response(200, None);
    let app = ForkLaunchRouter::anonymous()
        .with_config(config)
        .get(
            route,
            |Extension(context): Extension<RequestContext>,
             Extension(session): Extension<AuthenticatedSession>| async move {
                Json(
                    json!({"correlation":context.correlation_id,"actor":session.principal.subject}),
                )
            },
        )
        .into_router();
    for (name, value) in [
        ("x-correlation-id", Some("explicit-correlation")),
        ("x-request-id", Some("request-id-fallback")),
        ("x-correlation-id", None),
    ] {
        let mut builder = Request::builder().uri("/context");
        if let Some(value) = value {
            builder = builder.header(name, value);
        }
        let mut request = builder.body(Body::empty()).unwrap();
        request
            .extensions_mut()
            .insert(RequestContext::new("stale-extension"));
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let propagated = response
            .headers()
            .get("x-request-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        let authenticated = captured.lock().unwrap().last().unwrap().clone();
        assert_eq!(body["actor"], "verified-actor");
        assert_eq!(body["correlation"], authenticated.correlation_id);
        if let Some(value) = value {
            assert_eq!(body["correlation"], value);
        } else {
            assert!(propagated.starts_with("forklaunch-"));
            assert_eq!(body["correlation"], propagated);
        }
        assert_ne!(body["correlation"], "stale-extension");
    }
}

#[tokio::test]
async fn public_handlers_also_receive_request_context() {
    let app = ForkLaunchRouter::anonymous()
        .get(
            RouteContract::get("PublicContext", "Public context", "/public")
                .with_response(200, None),
            |Extension(context): Extension<RequestContext>| async move { context.correlation_id },
        )
        .into_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/public")
                .header("x-correlation-id", "public-correlation")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(response.into_body(), 4096).await.unwrap(),
        "public-correlation"
    );
}
