use super::*;
use forklaunch_core::contract::{BodyContract, HttpMethod};
use forklaunch_core::http::RequestContext;
use std::{
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Poll},
};

fn route() -> RouteContract {
    RouteContract::get("Response", "Response", "/response")
        .with_response(200, Some(BodyContract::json("Output")))
}
fn request() -> FrameworkRequest {
    FrameworkRequest::new(
        HttpMethod::Get,
        "/response",
        RequestContext::new("response-test"),
    )
}

#[test]
fn registry_rejects_unsupported_and_stale_contracts() {
    let mut validators = ResponseValidators::default();
    for kind in [
        BodyKind::File,
        BodyKind::Binary,
        BodyKind::Multipart,
        BodyKind::ServerSentEvent,
        BodyKind::Unknown,
        BodyKind::UrlEncoded,
    ] {
        let mut contract = route();
        contract.responses[0].body.as_mut().unwrap().kind = kind;
        assert!(
            validators
                .insert(&contract, 10, Duration::from_secs(1), |_, _| None)
                .is_err()
        );
    }
    assert!(
        validators
            .insert(&route(), 0, Duration::from_secs(1), |_, _| None)
            .is_err()
    );
    assert!(
        validators
            .insert(&route(), 10, Duration::ZERO, |_, _| None)
            .is_err()
    );
    validators
        .insert(&route(), 10, Duration::from_secs(1), |_, _| None)
        .unwrap();
    assert!(
        validators
            .insert(&route(), 10, Duration::from_secs(1), |_, _| None)
            .is_err()
    );
    assert!(validators.validate_routes(&[]).is_err());
    assert!(validators.validate_routes(&[route(), route()]).is_err());
    assert!(
        validators
            .validate_routes(&[route().with_response(500, None)])
            .is_err()
    );
    assert!(validators.validate_routes(&[route()]).is_ok());
}

#[tokio::test]
async fn success_preserves_headers_extensions_and_bytes() {
    let mut validators = ResponseValidators::default();
    validators
        .insert(&route(), 64, Duration::from_secs(1), |_, response| {
            assert_eq!(response.body.as_text(), Some("{\"ok\":true}"));
            None
        })
        .unwrap();
    let mut response = Response::new(Body::from("{\"ok\":true}"));
    response
        .headers_mut()
        .append("set-cookie", "a=1".parse().unwrap());
    response
        .headers_mut()
        .append("set-cookie", "b=2".parse().unwrap());
    response.extensions_mut().insert(42_u64);
    let response = validators.validate(&route(), &request(), response).await;
    assert_eq!(response.headers().get_all("set-cookie").iter().count(), 2);
    assert_eq!(response.extensions().get::<u64>(), Some(&42));
    assert_eq!(
        to_bytes(response.into_body(), 64).await.unwrap(),
        "{\"ok\":true}"
    );
}

#[tokio::test]
async fn warning_none_and_unregistered_responses_are_not_buffered() {
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    let mut validators = ResponseValidators::default();
    validators
        .insert(&route(), 1, Duration::from_millis(1), move |_, _| {
            calls.fetch_add(1, Ordering::SeqCst);
            None
        })
        .unwrap();
    for mode in [ValidationMode::Warning, ValidationMode::None] {
        let mut contract = route();
        contract.options.response_validation = mode;
        let response = validators
            .validate(
                &contract,
                &request(),
                Response::new(Body::from(vec![255_u8; 10])),
            )
            .await;
        assert_eq!(response.status(), 200);
        assert_eq!(to_bytes(response.into_body(), 10).await.unwrap().len(), 10);
    }
    let response = ResponseValidators::default()
        .validate(
            &route(),
            &request(),
            Response::new(Body::from(vec![255_u8])),
        )
        .await;
    assert_eq!(response.status(), 200);
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

struct Unreadable(bool);
impl http_body::Body for Unreadable {
    type Data = bytes::Bytes;
    type Error = std::io::Error;
    fn poll_frame(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        if self.0 {
            Poll::Pending
        } else {
            Poll::Ready(Some(Err(std::io::Error::other("PRIVATE_RESPONSE_DETAIL"))))
        }
    }
}

#[tokio::test]
async fn read_limit_utf8_io_and_timeout_failures_are_safe() {
    let mut validators = ResponseValidators::default();
    validators
        .insert(&route(), 4, Duration::from_millis(5), |_, _| {
            panic!("failed read reached validator")
        })
        .unwrap();
    for body in [
        Body::from("PRIVATE_PAYLOAD"),
        Body::from(vec![255_u8]),
        Body::new(Unreadable(false)),
        Body::new(Unreadable(true)),
    ] {
        let response = validators
            .validate(&route(), &request(), Response::new(body))
            .await;
        assert_eq!(response.status(), 500);
        let bytes = to_bytes(response.into_body(), 1024).await.unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains("internal server error"));
        assert!(text.contains("response-test"));
        assert!(!text.contains("PRIVATE"));
    }
}

#[tokio::test]
async fn replacement_preserves_metadata_without_stale_body_headers() {
    let mut validators = ResponseValidators::default();
    validators
        .insert(&route(), 64, Duration::from_secs(1), |_, _| {
            Some(FrameworkResponse::text(500, "public diagnostic"))
        })
        .unwrap();
    let mut response = Response::new(Body::from("42"));
    response
        .headers_mut()
        .insert("x-result", "valid".parse().unwrap());
    response
        .headers_mut()
        .insert("content-length", "2".parse().unwrap());
    response
        .headers_mut()
        .insert("etag", "old".parse().unwrap());
    for name in [
        "digest",
        "content-digest",
        "repr-digest",
        "content-md5",
        "trailer",
        "last-modified",
    ] {
        response.headers_mut().insert(
            axum::http::HeaderName::from_static(name),
            "stale".parse().unwrap(),
        );
    }
    response
        .headers_mut()
        .append("set-cookie", "a=1".parse().unwrap());
    response
        .headers_mut()
        .append("set-cookie", "b=2".parse().unwrap());
    response.extensions_mut().insert(42_u64);
    let response = validators.validate(&route(), &request(), response).await;
    assert_eq!(response.status(), 500);
    for name in [
        "digest",
        "content-digest",
        "repr-digest",
        "content-md5",
        "trailer",
        "last-modified",
    ] {
        assert!(!response.headers().contains_key(name));
    }
    assert_eq!(response.headers().get_all("set-cookie").iter().count(), 2);
    assert_eq!(response.headers()["x-result"], "valid");
    assert!(!response.headers().contains_key("content-length"));
    assert!(!response.headers().contains_key("etag"));
    assert_eq!(response.extensions().get::<u64>(), Some(&42));
}

#[tokio::test]
async fn head_preserves_get_headers_without_validating_a_stripped_body() {
    use crate::{ForkLaunchRouter, RouterConfig};
    use forklaunch_core::ServiceInfo;
    use tower::ServiceExt;
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let mut validators = ResponseValidators::default();
    validators
        .insert(&route(), 64, Duration::from_secs(1), move |_, response| {
            count.fetch_add(1, Ordering::SeqCst);
            (response.body.as_text() != Some("payload"))
                .then(|| FrameworkResponse::text(500, "missing body"))
        })
        .unwrap();
    let router = ForkLaunchRouter::new(ServiceInfo::new("head", "1"))
        .get(route(), || async { ([("x-result", "valid")], "payload") })
        .with_config(RouterConfig {
            response_validators: validators,
            ..RouterConfig::default()
        })
        .into_router();
    for method in ["HEAD", "GET"] {
        let response = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method(method)
                    .uri("/response")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["x-result"], "valid");
        let bytes = to_bytes(response.into_body(), 64).await.unwrap();
        assert_eq!(
            bytes.as_ref(),
            if method == "HEAD" {
                b"".as_slice()
            } else {
                b"payload".as_slice()
            }
        );
        assert_eq!(calls.load(Ordering::SeqCst), usize::from(method == "GET"));
    }
}

#[tokio::test]
async fn bodyless_statuses_bypass_callbacks_in_standalone_and_mixed_contracts() {
    use crate::{ForkLaunchRouter, RouterConfig};
    use forklaunch_core::ServiceInfo;
    use tower::ServiceExt;
    for status in [204, 205, 304] {
        for mixed in [false, true] {
            let contract = if mixed {
                route()
            } else {
                RouteContract::get("Response", "Response", "/response")
            }
            .with_response(status, None);
            let mut validators = ResponseValidators::default();
            validators
                .insert(&contract, 64, Duration::from_secs(1), |_, _| {
                    panic!("bodyless status reached callback")
                })
                .unwrap();
            let router = ForkLaunchRouter::new(ServiceInfo::new("bodyless", "1"))
                .get(contract, move || async move {
                    (
                        axum::http::StatusCode::from_u16(status).unwrap(),
                        [("x-result", "valid")],
                    )
                })
                .with_config(RouterConfig {
                    response_validators: validators,
                    ..RouterConfig::default()
                })
                .into_router();
            let response = router
                .oneshot(
                    axum::http::Request::builder()
                        .uri("/response")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), status);
            assert_eq!(response.headers()["x-result"], "valid");
            assert!(to_bytes(response.into_body(), 64).await.unwrap().is_empty());
        }
    }
}

#[test]
fn versioned_registration_is_explicit_and_checks_every_branch_snapshot() {
    use forklaunch_core::contract::{RequestContract, VersionedContract};
    let mut versioned = route();
    versioned.responses.clear();
    for (name, status) in [("first", 201), ("last", 202)] {
        versioned = versioned.with_version(VersionedContract {
            version: name.into(),
            request: RequestContract::default(),
            responses: vec![ResponseContract::new(
                status,
                Some(BodyContract::json("Output")),
            )],
        });
    }
    let mut empty_branch = versioned.clone();
    empty_branch
        .versions
        .get_mut("first")
        .unwrap()
        .responses
        .clear();
    assert!(
        ResponseValidators::default()
            .insert_versioned(&empty_branch, 16, Duration::from_secs(1), |_, _, _| None)
            .is_err()
    );
    let artifact = versioned.to_json();
    let mut validators = ResponseValidators::default();
    assert!(
        validators
            .insert(&versioned, 16, Duration::from_secs(1), |_, _| None)
            .is_err()
    );
    assert!(
        validators
            .insert_versioned(&route(), 16, Duration::from_secs(1), |_, _, _| None)
            .is_err()
    );
    validators
        .insert_versioned(&versioned, 16, Duration::from_secs(1), |_, _, _| None)
        .unwrap();
    validators.enable_warning(&versioned).unwrap();
    assert!(validators.validate_routes(&[versioned.clone()]).is_ok());
    assert_eq!(versioned.to_json(), artifact);
    let mut stale = versioned.clone();
    stale.versions.get_mut("first").unwrap().responses[0].status_code = 203;
    assert!(validators.validate_routes(&[stale.clone()]).is_err());
    assert!(validators.enable_warning(&stale).is_err());
    let mut streaming = versioned;
    streaming.versions.get_mut("first").unwrap().responses[0]
        .body
        .as_mut()
        .unwrap()
        .kind = BodyKind::Binary;
    assert!(
        ResponseValidators::default()
            .insert_versioned(&streaming, 16, Duration::from_secs(1), |_, _, _| None)
            .is_err()
    );
}

#[test]
fn versioned_error_metadata_preserves_repeated_headers_and_discards_integrity() {
    let mut headers = axum::http::HeaderMap::new();
    headers.append("set-cookie", "a=1".parse().unwrap());
    headers.append("set-cookie", "b=2".parse().unwrap());
    headers.insert("x-selected", "last".parse().unwrap());
    for name in [
        "etag",
        "content-length",
        "digest",
        "content-digest",
        "repr-digest",
        "trailer",
        "last-modified",
    ] {
        headers.insert(name, "old".parse().unwrap());
    }
    let mut extensions = axum::http::Extensions::new();
    extensions.insert(42_u64);
    let mut response = framework_response_to_axum(
        FrameworkResponse::text(500, "safe").with_header("content-type", "text/plain"),
    );
    preserve_metadata(headers, extensions, &mut response);
    assert_eq!(response.headers().get_all("set-cookie").iter().count(), 2);
    assert_eq!(response.headers()["x-selected"], "last");
    assert_eq!(response.headers()["content-type"], "text/plain");
    assert!(!response.headers().contains_key("etag"));
    assert!(!response.headers().contains_key("digest"));
    assert!(!response.headers().contains_key("content-length"));
    assert_eq!(response.extensions().get::<u64>(), Some(&42));
}

fn prehandler_app(
    local: bool,
    profile: bool,
    protected: bool,
) -> (axum::Router, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    use crate::{ForkLaunchRouter, RouterConfig};
    use forklaunch_core::ServiceInfo;
    use forklaunch_core::contract::{
        AccessLevel, FieldContract, RequestContract, VersionedContract,
    };
    let mut query = FieldContract::required("match", "Match");
    query.schema = query.schema.with_json_schema("{\"const\":\"yes\"}");
    let mut body = BodyContract::json("Input");
    body.schema = body.schema.with_json_schema("{\"type\":\"object\",\"required\":[\"flag\"],\"properties\":{\"flag\":{\"const\":\"ok\"}}}");
    let route = RouteContract::post("Origin", "Origin", "/origin")
        .with_access(if protected {
            AccessLevel::Authenticated
        } else {
            AccessLevel::Public
        })
        .with_version(VersionedContract {
            version: "only".into(),
            request: RequestContract {
                query: vec![query],
                body: Some(body),
                ..RequestContract::default()
            },
            responses: vec![ResponseContract::new(
                200,
                Some(BodyContract::json("Output")),
            )],
        });
    let handlers = Arc::new(AtomicUsize::new(0));
    let callbacks = Arc::new(AtomicUsize::new(0));
    let mut validators = ResponseValidators::default();
    if profile {
        let callbacks = callbacks.clone();
        validators
            .insert_versioned(&route, 1024, Duration::from_secs(1), move |_, _, _| {
                callbacks.fetch_add(1, Ordering::SeqCst);
                Some(FrameworkResponse::text(500, "CALLBACK_MUST_NOT_RUN"))
            })
            .unwrap();
    }
    let counted = handlers.clone();
    let router = if local {
        let mut local = forklaunch_core::http::LocalRouter::new();
        local.register(route.clone(), move |_: FrameworkRequest| {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(FrameworkResponse::text(200, "handler"))
        });
        ForkLaunchRouter::from_local_router(ServiceInfo::new("origin", "1"), [route], local)
    } else {
        ForkLaunchRouter::new(ServiceInfo::new("origin", "1")).post(route, move || {
            let counted = counted.clone();
            async move {
                counted.fetch_add(1, Ordering::SeqCst);
                "handler"
            }
        })
    };
    let config = RouterConfig {
        response_validators: validators,
        ..RouterConfig::default()
    }
    .with_response_authenticator(|_, _| async {
        Err(ErrorResponse::new(ErrorKind::Unauthorized, "missing token"))
    });
    (
        router.with_config(config).into_router(),
        handlers,
        callbacks,
    )
}

#[tokio::test]
async fn versioned_profile_preserves_native_prehandler_errors_and_skips_callbacks() {
    use tower::ServiceExt;
    let cases = [
        (
            "match=invalid",
            "application/json",
            "{\"flag\":\"ok\"}",
            false,
        ),
        ("match=%FF", "application/json", "{\"flag\":\"ok\"}", false),
        ("", "application/json", "{\"flag\":\"ok\"}", false),
        ("match=yes", "application/json", "{", false),
        ("match=yes", "application/json", "{\"flag\":\"bad\"}", false),
        ("match=yes", "text/plain", "{\"flag\":\"ok\"}", false),
        ("match=yes", "application/json", "{\"flag\":\"ok\"}", true),
    ];
    for local in [false, true] {
        for (query, media, input, protected) in cases {
            let mut results = Vec::new();
            for profile in [false, true] {
                let (app, handlers, callbacks) = prehandler_app(local, profile, protected);
                let response = app
                    .oneshot(
                        axum::http::Request::builder()
                            .method("POST")
                            .uri(format!("/origin?{query}"))
                            .header("content-type", media)
                            .header("x-correlation-id", "origin-test")
                            .body(Body::from(input))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                let status = response.status();
                assert!(
                    status.is_client_error(),
                    "{local} {profile} {query} {media}"
                );
                results.push((status, to_bytes(response.into_body(), 8192).await.unwrap()));
                assert_eq!(handlers.load(Ordering::SeqCst), 0);
                assert_eq!(callbacks.load(Ordering::SeqCst), 0);
            }
            assert_eq!(results[0], results[1], "{local} {query} {media}");
        }
    }
}

#[tokio::test]
async fn converted_local_handler_errors_keep_redaction_without_callback() {
    use crate::{ForkLaunchRouter, HandlerErrorFormat, RouterConfig};
    use forklaunch_core::ServiceInfo;
    use forklaunch_core::contract::{RequestContract, VersionedContract};
    use tower::ServiceExt;
    let route =
        RouteContract::get("Failure", "Failure", "/failure").with_version(VersionedContract {
            version: "only".into(),
            request: RequestContract::default(),
            responses: vec![ResponseContract::new(
                200,
                Some(BodyContract::json("Output")),
            )],
        });
    let mut validators = ResponseValidators::default();
    validators
        .insert_versioned(&route, 1024, Duration::from_secs(1), |_, _, _| {
            panic!("converted handler error reached callback")
        })
        .unwrap();
    let mut local = forklaunch_core::http::LocalRouter::new();
    local.register(route.clone(), |_: FrameworkRequest| {
        Err(forklaunch_common::FrameworkError::new(
            "private",
            "PRIVATE_HANDLER_ERROR",
        ))
    });
    let app = ForkLaunchRouter::from_local_router(ServiceInfo::new("failure", "1"), [route], local)
        .with_config(RouterConfig {
            response_validators: validators,
            handler_error_format: HandlerErrorFormat::TypeScriptText,
            ..RouterConfig::default()
        })
        .into_router();
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/failure")
                .header("x-correlation-id", "failure-test")
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
    assert_eq!(
        to_bytes(response.into_body(), 1024).await.unwrap(),
        "Internal server error:\n\nCorrelation id: failure-test"
    );
}
