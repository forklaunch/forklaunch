use super::*;

#[test]
#[should_panic(expected = "mutually exclusive")]
fn conflicting_native_profile_rejected() {
    let _ = wrap(
        Router::new(),
        Some(tower_http::cors::CorsLayer::permissive()),
        Some(TypeScriptCors::default()),
    );
}

#[test]
#[should_panic(expected = "must be a final HTTP status")]
fn invalid_preflight_status_rejected() {
    let _ = wrap(
        Router::new(),
        None,
        Some(TypeScriptCors {
            options_success_status: StatusCode::CONTINUE,
            ..TypeScriptCors::default()
        }),
    );
}

use crate::{ForkLaunchRouter, ParserErrorFormat, RouterConfig, RouterOriginFormat};
use axum::body::{Body, to_bytes};
use forklaunch_core::contract::{HttpMethod, RouteContract};
use tower::ServiceExt;

#[tokio::test]
async fn configured_final_preflight_statuses_keep_empty_cors_response() {
    for status in [200, 205, 299, 300, 304, 400, 418, 500, 599] {
        let app = wrap(
            Router::new(),
            None,
            Some(TypeScriptCors {
                options_success_status: StatusCode::from_u16(status).unwrap(),
                ..TypeScriptCors::default()
            }),
        );
        let response = app
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/missing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status);
        assert_eq!(response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
        assert!(
            to_bytes(response.into_body(), 4096)
                .await
                .unwrap()
                .is_empty()
        );
    }
}

#[tokio::test]
async fn preflight_interception_disabled_continue_and_options_routing_are_explicit() {
    for policy in [
        None,
        Some(TypeScriptCors::default()),
        Some(TypeScriptCors {
            origin: TypeScriptCorsOrigin::Disabled,
            ..TypeScriptCors::default()
        }),
        Some(TypeScriptCors {
            preflight_continue: true,
            ..TypeScriptCors::default()
        }),
    ] {
        let intercept = policy
            .as_ref()
            .is_some_and(|config| !config.disabled() && !config.preflight_continue);
        let app = ForkLaunchRouter::anonymous()
            .get(
                RouteContract::get("Read", "Read", "/items/:id").with_response(200, None),
                || async { "ok" },
            )
            .route(
                RouteContract::new("Options", "Options", HttpMethod::Options, "/explicit")
                    .with_response(202, None),
                || async { (StatusCode::ACCEPTED, "explicit") },
            )
            .with_config(RouterConfig {
                typescript_cors: policy.clone(),
                router_origin_format: RouterOriginFormat::TypeScriptHtml,
                parser_error_format: ParserErrorFormat::TypeScriptText,
                ..RouterConfig::default()
            })
            .into_router();
        for (path, downstream_status) in [
            ("/items/one", 200),
            ("/items/%GG", 400),
            ("/explicit", 202),
            ("/missing", 404),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("OPTIONS")
                        .uri(path)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                response.status().as_u16(),
                if intercept { 204 } else { downstream_status }
            );
            if !intercept && path == "/items/one" {
                assert_eq!(response.headers()[header::ALLOW], "GET, HEAD");
                assert_eq!(response.headers()[header::CONTENT_TYPE], "text/plain");
                assert_eq!(
                    to_bytes(response.into_body(), 4096).await.unwrap(),
                    "GET, HEAD"
                );
            }
        }
    }
}

#[tokio::test]
async fn prepared_headers_support_explicit_vary_merge_and_preserve_overrides() {
    let app = Router::new().route(
        "/",
        axum::routing::get(
            |axum::Extension(prepared): axum::Extension<TypeScriptCorsHeaders>| async move {
                let mut response = "ok".into_response();
                response.headers_mut().extend(prepared.0);
                response
                    .headers_mut()
                    .append(header::VARY, HeaderValue::from_static("Accept-Encoding"));
                response.headers_mut().insert(
                    header::ACCESS_CONTROL_ALLOW_ORIGIN,
                    HeaderValue::from_static("application"),
                );
                response
            },
        ),
    );
    let app = wrap(
        app,
        None,
        Some(TypeScriptCors {
            origin: TypeScriptCorsOrigin::Reflect,
            ..TypeScriptCors::default()
        }),
    );
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::ORIGIN, "https://allowed.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
        "application"
    );
    assert_eq!(
        response
            .headers()
            .get_all(header::VARY)
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["Origin", "Accept-Encoding"]
    );
}

#[test]
#[should_panic(expected = "JavaScript-safe integer")]
fn max_age_that_javascript_would_round_is_rejected() {
    let _ = wrap(
        Router::new(),
        None,
        Some(TypeScriptCors {
            max_age: Some(u64::MAX),
            ..TypeScriptCors::default()
        }),
    );
}

#[tokio::test]
async fn origin_delegate_receives_joined_bytes_once_before_dispatch() {
    use std::sync::{Arc, Mutex};
    let observed = Arc::new(Mutex::new(Vec::new()));
    let calls = observed.clone();
    let callback = TypeScriptCorsOriginDelegate::new(move |origin| {
        calls.lock().unwrap().push(origin);
        async { Ok(TypeScriptCorsOriginDecision::Reflect) }
    });
    let app = wrap(
        Router::new().route("/", axum::routing::get(|| async { "ok" })),
        None,
        Some(TypeScriptCors {
            origin: TypeScriptCorsOrigin::Delegate(callback),
            ..TypeScriptCors::default()
        }),
    );
    for method in ["GET", "OPTIONS"] {
        let request = Request::builder()
            .method(method)
            .uri("/")
            .header(header::ORIGIN, "https://first.example")
            .header(header::ORIGIN, "https://second.example")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "https://first.example, https://second.example"
        );
        assert_eq!(
            response.status().as_u16(),
            if method == "GET" { 200 } else { 204 }
        );
    }
    let calls = observed.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert!(calls.iter().all(|value| value.as_ref().unwrap() == "https://first.example, https://second.example"));
}

#[tokio::test]
async fn canceled_origin_delegate_drops_pending_work_without_dispatch() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct Dropped(Arc<AtomicUsize>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let callback_drops = drops.clone();
    let dispatched = Arc::new(AtomicUsize::new(0));
    let handlers = dispatched.clone();
    let app = wrap(
        Router::new().route(
            "/",
            axum::routing::get(move || {
                let handlers = handlers.clone();
                async move {
                    handlers.fetch_add(1, Ordering::SeqCst);
                    "ok"
                }
            }),
        ),
        None,
        Some(TypeScriptCors {
            origin: TypeScriptCorsOrigin::Delegate(TypeScriptCorsOriginDelegate::new(
                move |origin| {
                    assert!(origin.is_none());
                    let guard = Dropped(callback_drops.clone());
                    async move {
                        let _guard = guard;
                        std::future::pending().await
                    }
                },
            )),
            ..TypeScriptCors::default()
        }),
    );
    let response = tokio::time::timeout(
        std::time::Duration::from_millis(10),
        app.oneshot(Request::builder().uri("/").body(Body::empty()).unwrap()),
    )
    .await;
    assert!(response.is_err());
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(dispatched.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn numeric_max_age_preserves_javascript_text_and_preflight_scope() {
    for (value, expected) in [
        (-0.0, "0"),
        (-0.5, "-0.5"),
        (0.5, "0.5"),
        (f64::NAN, "NaN"),
        (f64::INFINITY, "Infinity"),
        (f64::NEG_INFINITY, "-Infinity"),
        (1e21, "1e+21"),
    ] {
        let app = wrap(
            Router::new(),
            None,
            Some(TypeScriptCors {
                // The explicit Number field overrides the legacy integer field.
                max_age: Some(u64::MAX),
                max_age_number: Some(value),
                ..TypeScriptCors::default()
            }),
        );
        for method in ["GET", "OPTIONS"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri("/")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let age = response.headers().get(header::ACCESS_CONTROL_MAX_AGE);
            if method == "OPTIONS" {
                assert_eq!(age.unwrap(), expected);
            } else {
                assert!(age.is_none());
            }
        }
    }
}
