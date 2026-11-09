use super::*;
use crate::{ForkLaunchRouter, ResponseValidators, RouterConfig};
use axum::{
    http::{HeaderValue, Request},
    response::IntoResponse,
};
use forklaunch_core::ServiceInfo;
use forklaunch_core::contract::{BodyContract, HttpMethod};
use forklaunch_core::http::{LocalRouter, RequestContext};
use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};
use tower::ServiceExt;
fn route() -> RouteContract {
    RouteContract::get("Defaults", "Defaults", "/defaults")
        .with_response(200, Some(BodyContract::json("Output")))
        .with_response(401, None)
}
fn profiles() -> DefaultErrorResponses {
    let mut value = DefaultErrorResponses::default();
    value
        .insert(&route(), &[404, 500], 32, Duration::from_millis(20))
        .unwrap();
    value
}
fn request() -> FrameworkRequest {
    FrameworkRequest::new(
        HttpMethod::Get,
        "/defaults",
        RequestContext::new("default-test"),
    )
}
#[tokio::test]
async fn collected_error_bodies_have_finite_bounds_and_preserve_unrelated_metadata() {
    struct Pending;
    impl http_body::Body for Pending {
        type Data = bytes::Bytes;
        type Error = std::io::Error;
        fn poll_frame(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
        ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
            Poll::Pending
        }
    }
    for versioned in [false, true] {
        let mut contract = route();
        let selected = if versioned {
            let responses = contract.responses.clone();
            contract = contract.with_version(forklaunch_core::contract::VersionedContract {
                version: "only".into(),
                request: forklaunch_core::contract::RequestContract::default(),
                responses,
            });
            let mut selected = DefaultErrorResponses::default();
            selected
                .insert_versioned(&contract, &[404, 500], 32, Duration::from_millis(20))
                .unwrap();
            selected
        } else {
            profiles()
        };
        for (body, media, status) in [
            ("\"value\"", "application/json", 404),
            ("value", "text/plain", 404),
            ("", "text/plain", 404),
            ("{}", "application/json", 500),
            ("xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx", "text/plain", 500),
            ("PRIVATE", "application/octet-stream", 500),
            ("PRIVATE", "image/png", 500),
        ] {
            let mut response = (axum::http::StatusCode::NOT_FOUND, body).into_response();
            response
                .headers_mut()
                .insert("content-type", media.parse().unwrap());
            response
                .headers_mut()
                .append("set-cookie", HeaderValue::from_static("a=1"));
            response
                .headers_mut()
                .append("set-cookie", HeaderValue::from_static("b=2"));
            response
                .headers_mut()
                .insert("x-result", HeaderValue::from_static("keep"));
            response
                .headers_mut()
                .insert("etag", HeaderValue::from_static("stale"));
            response.extensions_mut().insert(42_u32);
            let response = normalize(&selected, &contract, &request(), response).await;
            assert_eq!(response.status().as_u16(), status);
            assert_eq!(response.headers().get_all("set-cookie").iter().count(), 2);
            assert_eq!(response.headers()["x-result"], "keep");
            assert_eq!(response.extensions().get::<u32>(), Some(&42));
            if !body.is_empty() {
                assert!(!response.headers().contains_key("etag"));
            }
            let bytes = to_bytes(response.into_body(), 8192).await.unwrap();
            assert!(!String::from_utf8_lossy(&bytes).contains("PRIVATE"));
            if body == "\"value\"" {
                assert_eq!(bytes, "value");
            }
        }
        for body in [Body::from(vec![0xff]), Body::new(Pending)] {
            let response = Response::builder()
                .status(404)
                .header("content-type", "text/plain")
                .body(body)
                .unwrap();
            let response = normalize(&selected, &contract, &request(), response).await;
            assert_eq!(response.status(), 500);
            assert_eq!(
                response.headers()["content-type"],
                "text/plain; charset=utf-8"
            );
        }
    }
}
#[tokio::test]
async fn local_and_axum_profiles_preserve_callbacks_explicit_statuses_and_exports() {
    for local in [false, true] {
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let contract = route();
        let mut validators = ResponseValidators::default();
        validators
            .insert(
                &contract,
                8192,
                Duration::from_secs(1),
                move |_, response| {
                    count.fetch_add(1, Ordering::SeqCst);
                    assert_eq!(
                        response.headers.get("x-result").map(String::as_str),
                        Some("keep")
                    );
                    None
                },
            )
            .unwrap();
        let child = if local {
            let mut handlers = LocalRouter::new();
            handlers.register(contract.clone(), |_: FrameworkRequest| {
                Ok(FrameworkResponse::json(404, "\"value\"").with_header("x-result", "keep"))
            });
            ForkLaunchRouter::from_local_router(
                ServiceInfo::new("local", "1"),
                [contract],
                handlers,
            )
        } else {
            ForkLaunchRouter::new(ServiceInfo::new("axum", "1")).get(contract, || async {
                (
                    axum::http::StatusCode::NOT_FOUND,
                    [("x-result", "keep")],
                    axum::Json("value"),
                )
            })
        };
        let child = child.with_config(RouterConfig {
            default_error_responses: profiles(),
            response_validators: validators,
            ..RouterConfig::default()
        });
        let router = ForkLaunchRouter::new(ServiceInfo::new("parent", "1")).nest("/nested", child);
        let responses = &router.registry().routes()[0].responses;
        assert!(
            responses
                .iter()
                .find(|response| response.status_code == 401)
                .unwrap()
                .body
                .is_none()
        );
        assert_eq!(responses.len(), 4);
        assert!(
            router
                .registry()
                .clone()
                .into_artifact()
                .to_json()
                .contains("text/plain")
        );
        let response = router
            .into_router()
            .oneshot(
                Request::builder()
                    .uri("/nested/defaults")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 404);
        assert_eq!(
            response.headers()["content-type"],
            "text/plain; charset=utf-8"
        );
        assert_eq!(to_bytes(response.into_body(), 8192).await.unwrap(), "value");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn zero_byte_wire_admission_matches_local_without_claiming_original_value_types() {
    for media in [
        None,
        Some("text/plain"),
        Some("application/json"),
        Some("application/octet-stream"),
    ] {
        for encoded in [false, true] {
            for body in [
                FrameworkBody::Empty,
                FrameworkBody::Text(String::new()),
                FrameworkBody::Binary(vec![]),
            ] {
                let mut original = FrameworkResponse {
                    status_code: 404,
                    headers: std::collections::BTreeMap::default(),
                    body,
                };
                if let Some(media) = media {
                    original.headers.insert("content-type".into(), media.into());
                }
                if encoded {
                    original
                        .headers
                        .insert("content-encoding".into(), "gzip".into());
                }
                let local = profiles()
                    .normalize(&route(), original.clone(), "test")
                    .unwrap();
                let mut expected = original.clone();
                if media == Some("text/plain") {
                    expected
                        .headers
                        .insert("content-type".into(), "text/plain; charset=utf-8".into());
                }
                assert_eq!(local, expected);
                let transport = framework_response_to_axum(original);
                let mut before = transport.headers().clone();
                if media == Some("text/plain") {
                    before.insert(
                        "content-type",
                        HeaderValue::from_static("text/plain; charset=utf-8"),
                    );
                }
                let result = normalize(&profiles(), &route(), &request(), transport).await;
                assert_eq!(result.status(), 404);
                assert_eq!(result.headers(), &before);
                assert!(to_bytes(result.into_body(), 1024).await.unwrap().is_empty());
            }
        }
    }
    let result = normalize(
        &profiles(),
        &route(),
        &request(),
        framework_response_to_axum(FrameworkResponse::json(404, "\"\"")),
    )
    .await;
    assert_eq!(
        result.headers()["content-type"],
        "text/plain; charset=utf-8"
    );
    assert!(to_bytes(result.into_body(), 1024).await.unwrap().is_empty());
}

#[tokio::test]
async fn default_acceptance_does_not_bypass_header_validation_or_configuration_reset() {
    let contract = route();
    let mut validators = ResponseValidators::default();
    validators
        .insert(&contract, 8192, Duration::from_secs(1), |_, response| {
            (!response.headers.contains_key("x-required"))
                .then(|| FrameworkResponse::text(500, "required header absent"))
        })
        .unwrap();
    let router = ForkLaunchRouter::new(ServiceInfo::new("headers", "1"))
        .get(contract, || async {
            (axum::http::StatusCode::NOT_FOUND, "valid string")
        })
        .with_config(RouterConfig {
            default_error_responses: profiles(),
            response_validators: validators,
            ..RouterConfig::default()
        });
    let response = router
        .into_router()
        .oneshot(
            Request::builder()
                .uri("/defaults")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 500);
    assert_eq!(
        to_bytes(response.into_body(), 8192).await.unwrap(),
        "required header absent"
    );
    let router = ForkLaunchRouter::new(ServiceInfo::new("reset", "1"))
        .get(route(), || async { "ok" })
        .with_config(RouterConfig {
            default_error_responses: profiles(),
            ..RouterConfig::default()
        });
    assert_eq!(router.registry().routes()[0].responses.len(), 4);
    let router = router.with_config(RouterConfig::default());
    assert_eq!(router.registry().routes()[0].responses.len(), 2);
    let mut conflict = crate::RequestValidators::default();
    conflict
        .insert_with_rejection(
            &route(),
            forklaunch_core::contract::ResponseContract::new(404, None),
            |_, _| None,
        )
        .unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ForkLaunchRouter::new(
            ServiceInfo::new("conflict", "1")
        )
        .get(route(), || async { "ok" })
        .with_config(RouterConfig {
            default_error_responses: profiles(),
            request_validators: conflict,
            ..RouterConfig::default()
        })))
        .is_err()
    );
}

#[test]
fn known_middleware_json_envelopes_cannot_be_projected_as_only_text() {
    for status in [400, 401, 403] {
        let contract = RouteContract::get("Guard", "Guard", "/guard").with_response(200, None);
        let mut profile = DefaultErrorResponses::default();
        profile
            .insert(&contract, &[status], 32, Duration::from_secs(1))
            .unwrap();
        let config = RouterConfig {
            default_error_responses: profile,
            ..RouterConfig::default()
        };
        assert!(validate_configuration(&config, std::slice::from_ref(&contract)).is_err());
        let config = RouterConfig {
            auth_error_format: crate::AuthErrorFormat::TypeScriptText,
            parser_error_format: forklaunch_core::http::ParserErrorFormat::TypeScriptText,
            ..config
        };
        assert!(validate_configuration(&config, std::slice::from_ref(&contract)).is_ok());
        if status == 400 {
            let mut protected = contract.clone();
            protected.access = forklaunch_core::contract::AccessLevel::Authenticated;
            let json_auth = RouterConfig {
                auth_error_format: crate::AuthErrorFormat::Json,
                ..config.clone()
            };
            assert!(validate_configuration(&json_auth, &[protected]).is_err());
            let mut with_callback = config.clone();
            with_callback
                .request_validators
                .insert(&contract, |_, _| Some(FrameworkResponse::json(400, "{}")))
                .unwrap();
            assert!(
                validate_configuration(&with_callback, std::slice::from_ref(&contract)).is_err()
            );
            let changed = contract.with_query(forklaunch_core::contract::FieldContract::required(
                "required", "String",
            ));
            assert!(validate_configuration(&config, &[changed]).is_err());
        }
    }
}

#[tokio::test]
async fn selected500_stays_text_after_callback_failure_or_local_handler_error() {
    for local in [false, true] {
        let contract = route();
        let router = if local {
            let mut handlers = LocalRouter::new();
            handlers.register(contract.clone(), |_: FrameworkRequest| {
                Err(forklaunch_core::common::FrameworkError::new(
                    "private", "PRIVATE",
                ))
            });
            ForkLaunchRouter::from_local_router(
                ServiceInfo::new("local", "1"),
                [contract.clone()],
                handlers,
            )
        } else {
            ForkLaunchRouter::new(ServiceInfo::new("axum", "1")).get(contract.clone(), || async {
                (axum::http::StatusCode::NOT_FOUND, "valid")
            })
        };
        let mut callbacks = ResponseValidators::default();
        callbacks
            .insert(&contract, 1, Duration::from_secs(1), |_, _| None)
            .unwrap();
        let app = router
            .with_config(RouterConfig {
                default_error_responses: profiles(),
                response_validators: callbacks,
                ..RouterConfig::default()
            })
            .into_router();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/defaults")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 500);
        assert_eq!(
            response.headers()["content-type"],
            "text/plain; charset=utf-8"
        );
        assert!(
            !String::from_utf8_lossy(&to_bytes(response.into_body(), 8192).await.unwrap())
                .contains("PRIVATE")
        );
    }
}

#[tokio::test]
async fn selected_errors_cover_auth_and_request_validator_early_returns() {
    for kind in [
        forklaunch_core::http::ErrorKind::Internal,
        forklaunch_core::http::ErrorKind::Validation,
    ] {
        let mut contract = route();
        contract.access = forklaunch_core::contract::AccessLevel::Authenticated;
        let status = kind.status_code();
        let mut profiles = DefaultErrorResponses::default();
        profiles
            .insert(&contract, &[status], 1024, Duration::from_secs(1))
            .unwrap();
        let config = RouterConfig {
            default_error_responses: profiles,
            auth_error_format: if status == 400 {
                crate::AuthErrorFormat::TypeScriptText
            } else {
                crate::AuthErrorFormat::Json
            },
            parser_error_format: forklaunch_core::http::ParserErrorFormat::TypeScriptText,
            ..RouterConfig::default()
        }
        .with_response_authenticator(move |_, _| async move {
            Err(forklaunch_core::http::ErrorResponse::new(
                kind,
                "public failure",
            ))
        });
        let app = ForkLaunchRouter::new(ServiceInfo::new("auth", "1"))
            .get(contract, || async { "handler" })
            .with_config(config)
            .into_router();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/defaults")
                    .header("x-correlation-id", "early-test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status);
        assert_eq!(
            response.headers()["content-type"],
            "text/plain; charset=utf-8"
        );
    }
    let mut validators = crate::RequestValidators::default();
    validators
        .insert_with_rejection(
            &route(),
            forklaunch_core::contract::ResponseContract::new(400, None),
            |_, _| Some(FrameworkResponse::text(400, "PRIVATE")),
        )
        .unwrap();
    let app = ForkLaunchRouter::new(ServiceInfo::new("request", "1"))
        .get(route(), || async { "handler" })
        .with_config(RouterConfig {
            default_error_responses: profiles(),
            request_validators: validators,
            ..RouterConfig::default()
        })
        .into_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/defaults")
                .header("x-correlation-id", "early-test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 500);
    assert_eq!(
        response.headers()["content-type"],
        "text/plain; charset=utf-8"
    );
    let body = to_bytes(response.into_body(), 8192).await.unwrap();
    assert!(!String::from_utf8_lossy(&body).contains("PRIVATE"));
    assert!(String::from_utf8_lossy(&body).contains("early-test"));
}
struct Failing;
impl http_body::Body for Failing {
    type Data = bytes::Bytes;
    type Error = std::io::Error;
    fn poll_frame(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        Poll::Ready(Some(Err(std::io::Error::other("PRIVATE"))))
    }
}
#[tokio::test]
async fn selected400_body_read_failure_on_get_without_body_contract_is_text() {
    let mut selected = DefaultErrorResponses::default();
    selected
        .insert(&route(), &[400], 1024, Duration::from_secs(1))
        .unwrap();
    let app = ForkLaunchRouter::new(ServiceInfo::new("read", "1"))
        .get(route(), || async { "handler" })
        .with_config(RouterConfig {
            default_error_responses: selected,
            parser_error_format: forklaunch_core::http::ParserErrorFormat::TypeScriptText,
            ..RouterConfig::default()
        })
        .into_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/defaults")
                .header("x-correlation-id", "read-test")
                .body(Body::new(Failing))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    assert_eq!(
        response.headers()["content-type"],
        "text/plain; charset=utf-8"
    );
    assert_eq!(
        to_bytes(response.into_body(), 8192).await.unwrap(),
        "Request could not be processed:\n\nCorrelation id: read-test"
    );
}
