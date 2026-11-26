use super::*;
use crate::{
    AuthErrorFormat, ForkLaunchRouter, ParserErrorFormat, RequestContext, RouterConfig,
    RouterOriginFormat,
};
use axum::{
    Extension,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use forklaunch_core::contract::{AccessLevel, FieldContract, RouteContract};
use tower::ServiceExt;

#[test]
fn selection_joins_fields_decodes_latin1_and_generates_fresh_uuid_for_empty() {
    let mut headers = HeaderMap::new();
    headers.append("x-correlation-id", HeaderValue::from_static(" \tfirst\t "));
    headers.append(
        "x-correlation-id",
        HeaderValue::from_bytes(b"caf\xe9").unwrap(),
    );
    let selected = select(&headers);
    assert_eq!(selected.context, "first, café");
    assert_eq!(selected.wire.as_bytes(), b"first, caf\xe9");
    headers.clear();
    headers.insert("x-request-id", HeaderValue::from_static("independent"));
    let first = select(&headers);
    assert_eq!(
        uuid::Uuid::parse_str(&first.context)
            .unwrap()
            .get_version_num(),
        4
    );
    headers.insert("x-correlation-id", HeaderValue::from_static(" \t"));
    assert_ne!(select(&headers).context, first.context);
    headers.append("x-correlation-id", HeaderValue::from_static("second"));
    assert_eq!(select(&headers).context, ", second");
}

#[tokio::test]
async fn context_propagation_preserves_headers_overrides_and_native_default() {
    for format in [CorrelationIdFormat::Native, CorrelationIdFormat::TypeScript] {
        let app =
            ForkLaunchRouter::anonymous()
                .get(
                    RouteContract::get("Context", "Context", "/context").with_response(200, None),
                    |Extension(context): Extension<RequestContext>| async move {
                        context.correlation_id
                    },
                )
                .get(
                    RouteContract::get("Override", "Override", "/override")
                        .with_response(200, None),
                    || async { ([("x-correlation-id", "application")], "ok") },
                )
                .get(
                    RouteContract::get("Required", "Required", "/required")
                        .with_header(FieldContract::required("x-correlation-id", "String"))
                        .with_response(200, None),
                    || async { "must not run" },
                )
                .with_config(RouterConfig {
                    correlation_id_format: format,
                    ..RouterConfig::default()
                })
                .into_router();
        for path in ["/context", "/override", "/required", "/missing"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .header("x-request-id", "independent")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.headers()["x-request-id"], "independent");
            let correlation = response
                .headers()
                .get("x-correlation-id")
                .map(|header| header.to_str().unwrap().to_owned());
            if path == "/override" {
                assert_eq!(correlation.as_deref(), Some("application"));
            } else if format == CorrelationIdFormat::TypeScript {
                assert_eq!(
                    uuid::Uuid::parse_str(correlation.as_ref().unwrap())
                        .unwrap()
                        .get_version_num(),
                    4
                );
            } else {
                assert!(correlation.is_none());
            }
            if path == "/required" {
                assert_eq!(response.status(), 400);
            }
            let body = to_bytes(response.into_body(), 4096).await.unwrap();
            if path == "/context" {
                assert_eq!(body, correlation.unwrap_or_else(|| "independent".into()));
            }
        }
    }
}

#[tokio::test]
async fn auth_callback_parser_and_html_share_selected_context_with_exact_wire_encoding() {
    let app = ForkLaunchRouter::anonymous()
        .get(
            RouteContract::get("Auth", "Auth", "/auth/:id")
                .with_access(AccessLevel::Authenticated)
                .with_response(200, None),
            || async { StatusCode::OK },
        )
        .with_config(
            RouterConfig {
                correlation_id_format: CorrelationIdFormat::TypeScript,
                router_origin_format: RouterOriginFormat::TypeScriptHtml,
                parser_error_format: ParserErrorFormat::TypeScriptText,
                auth_error_format: AuthErrorFormat::Json,
                ..RouterConfig::default()
            }
            .with_authenticator(|request, _| async move {
                assert_eq!(request.context.correlation_id, "café, second");
                assert_eq!(
                    request.headers.get("x-correlation-id").map(String::as_str),
                    Some("café, second")
                );
                Err(forklaunch_core::auth::AuthError::MissingCredentials)
            }),
        )
        .into_router();
    for method in ["GET", "HEAD"] {
        for path in ["/auth/one", "/auth/%GG", "/missing"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .header(
                            "x-correlation-id",
                            HeaderValue::from_bytes(b"caf\xe9").unwrap(),
                        )
                        .header("x-correlation-id", "second")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let expected = if path == "/missing" && method == "GET" {
                "café, second".as_bytes()
            } else {
                b"caf\xe9, second"
            };
            assert_eq!(response.headers()["x-correlation-id"].as_bytes(), expected);
            let body = to_bytes(response.into_body(), 4096).await.unwrap();
            if method == "HEAD" {
                assert!(body.is_empty());
            } else if path == "/auth/%GG" {
                assert!(String::from_utf8_lossy(&body).contains("No correlation ID"));
            } else if path == "/auth/one" {
                assert!(String::from_utf8_lossy(&body).contains("café, second"));
            }
        }
    }
}

#[tokio::test]
async fn supplied_empty_and_obs_text_remain_present_for_required_header_validation() {
    let app = ForkLaunchRouter::anonymous()
        .get(
            RouteContract::get("Required", "Required", "/required")
                .with_header(FieldContract::required("x-correlation-id", "String"))
                .with_response(200, None),
            || async { "accepted" },
        )
        .with_config(RouterConfig {
            correlation_id_format: CorrelationIdFormat::TypeScript,
            ..RouterConfig::default()
        })
        .into_router();
    for header in [
        HeaderValue::from_static(""),
        HeaderValue::from_bytes(b"caf\xe9").unwrap(),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/required")
                    .header("x-correlation-id", header)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
    }
}
