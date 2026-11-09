//! Verified sessions shared by native Axum and local framework handlers.
use axum::{
    extract::FromRequestParts,
    http::request::Parts,
    response::{IntoResponse, Response},
};
use forklaunch_core::auth::{AuthError, Principal};
use forklaunch_core::contract::RouteContract;
use forklaunch_core::http::FrameworkRequest;
use serde::de::DeserializeOwned;
use std::{future::Future, pin::Pin, sync::Arc};

/// Returned only after credential verification, with application-enriched claims.
#[derive(Debug, Clone)]
pub struct AuthenticatedSession {
    pub principal: Principal,
    pub claims: serde_json::Value,
}

impl AuthenticatedSession {
    #[must_use]
    pub fn new(principal: Principal, claims: serde_json::Value) -> Self {
        Self { principal, claims }
    }
}

pub type AuthenticationFuture =
    Pin<Box<dyn Future<Output = Result<AuthenticatedSession, AuthError>> + Send>>;
pub type Authenticator =
    Arc<dyn Fn(FrameworkRequest, RouteContract) -> AuthenticationFuture + Send + Sync>;

/// Credential verification with application-selected safe HTTP errors.
pub type ResponseAuthenticationFuture = Pin<
    Box<
        dyn Future<Output = Result<AuthenticatedSession, forklaunch_core::http::ErrorResponse>>
            + Send,
    >,
>;
pub type ResponseAuthenticator =
    Arc<dyn Fn(FrameworkRequest, RouteContract) -> ResponseAuthenticationFuture + Send + Sync>;

/// Application-owned authorization for an already verified, enriched session.
pub type Authorizer = Arc<
    dyn Fn(
            &AuthenticatedSession,
            &RouteContract,
        ) -> Result<(), forklaunch_core::http::ErrorResponse>
        + Send
        + Sync,
>;

/// Extracts typed, verified session claims; client headers cannot populate it.
#[derive(Debug)]
pub struct Session<T>(pub T);

impl<T: DeserializeOwned + Send> Session<T> {
    fn from_verified_parts(parts: &Parts) -> Result<Self, (axum::http::StatusCode, &'static str)> {
        let reject = || (axum::http::StatusCode::UNAUTHORIZED, "invalid session");
        let session = parts
            .extensions
            .get::<AuthenticatedSession>()
            .ok_or_else(reject)?;
        serde_json::from_value(session.claims.clone())
            .map(Self)
            .map_err(|_| reject())
    }
}
impl<S, T> FromRequestParts<S> for Session<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = Response;
    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(Self::from_verified_parts(parts).map_err(IntoResponse::into_response))
    }
}

#[cfg(all(test, feature = "adapter"))]
mod tests {
    use super::*;
    use crate::{ForkLaunchRouter, RouterConfig};
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use forklaunch_core::auth::{Principal, parse_bearer, verify_jwt_hs256};
    use forklaunch_core::contract::{AccessLevel, RbacConfig, ValidationMode};
    use tower::ServiceExt;

    fn protected() -> RouteContract {
        let mut contract = RouteContract::get("Profile", "Profile", "/profile")
            .with_access(AccessLevel::Protected)
            .with_rbac(RbacConfig {
                allowed_roles: vec!["admin".to_owned()],
                ..RbacConfig::default()
            })
            .with_response(200, None);
        // Disabling validation must never disable security.
        contract.options.request_validation = ValidationMode::None;
        contract
    }

    fn app(config: RouterConfig) -> axum::Router {
        ForkLaunchRouter::anonymous()
            .get(
                protected(),
                |Session::<serde_json::Value>(claims)| async move { axum::Json(claims) },
            )
            .with_config(config)
            .into_router()
    }

    #[tokio::test]
    async fn auth_wire_format_is_explicit_and_applies_to_verification_and_policy_errors() {
        use crate::AuthErrorFormat;
        use forklaunch_core::http::{ErrorKind, ErrorResponse};
        for format in [AuthErrorFormat::Json, AuthErrorFormat::TypeScriptText] {
            for kind in [
                ErrorKind::Validation,
                ErrorKind::Unauthorized,
                ErrorKind::Forbidden,
                ErrorKind::Internal,
            ] {
                for policy_error in [false, true] {
                    let config = RouterConfig {
                        auth_error_format: format,
                        ..RouterConfig::default()
                    }
                    .with_response_authenticator(move |_, _| async move {
                        if policy_error {
                            Ok(AuthenticatedSession::new(
                                Principal::new("verified"),
                                serde_json::json!({}),
                            ))
                        } else {
                            Err(ErrorResponse::new(kind, "Public érror.\nSecond line."))
                        }
                    })
                    .with_authorizer(move |_, _| {
                        Err(ErrorResponse::new(kind, "Public érror.\nSecond line."))
                    });
                    let router = app(config);
                    for method in ["GET", "HEAD"] {
                        let response = router
                            .clone()
                            .oneshot(
                                Request::builder()
                                    .method(method)
                                    .uri("/profile")
                                    .header("x-correlation-id", "wire-test")
                                    .body(Body::empty())
                                    .unwrap(),
                            )
                            .await
                            .unwrap();
                        assert_eq!(response.status().as_u16(), kind.status_code());
                        assert_eq!(
                            response.headers()["content-type"],
                            match format {
                                AuthErrorFormat::Json => "application/json",
                                AuthErrorFormat::TypeScriptText => "text/plain; charset=utf-8",
                            }
                        );
                        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
                        if method == "HEAD" {
                            assert!(bytes.is_empty());
                            continue;
                        }
                        match format {
                            AuthErrorFormat::Json => {
                                let body: serde_json::Value =
                                    serde_json::from_slice(&bytes).unwrap();
                                assert_eq!(body["message"], "Public érror.\nSecond line.");
                                assert_eq!(body["correlationId"], "wire-test");
                            }
                            AuthErrorFormat::TypeScriptText => {
                                assert_eq!(
                                    bytes.as_ref(),
                                    "Public érror.\nSecond line.".as_bytes()
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn text_auth_errors_do_not_change_request_validation_errors() {
        let config = RouterConfig {
            auth_error_format: crate::AuthErrorFormat::TypeScriptText,
            ..RouterConfig::default()
        }
        .with_authenticator(|_, _| async {
            Ok(AuthenticatedSession::new(
                Principal::new("verified"),
                serde_json::json!({}),
            ))
        });
        let router = ForkLaunchRouter::anonymous()
            .get(
                RouteContract::get("Query", "Query", "/query")
                    .with_access(AccessLevel::Authenticated)
                    .with_query(forklaunch_core::contract::FieldContract::required(
                        "required", "String",
                    )),
                || async { "must not reach handler" },
            )
            .with_config(config)
            .into_router();
        let response = router
            .oneshot(Request::get("/query").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response.headers()["content-type"], "application/json");
    }

    #[tokio::test]
    async fn custom_authorizer_runs_only_after_verification_and_overrides_native_rbac() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let config = RouterConfig::default()
            .with_authenticator(|request, _| async move {
                match request.headers.get("authorization").map(String::as_str) {
                    Some("verified") => Ok(AuthenticatedSession::new(
                        Principal {
                            permissions: vec!["read".into()],
                            ..Principal::new("allowed")
                        },
                        serde_json::json!({"sub":"allowed"}),
                    )),
                    Some("verified-denied") => Ok(AuthenticatedSession::new(
                        Principal::new("denied"),
                        serde_json::json!({"sub":"denied"}),
                    )),
                    _ => Err(AuthError::MissingCredentials),
                }
            })
            .with_authorizer(move |session, contract| {
                observed.fetch_add(1, Ordering::SeqCst);
                assert_eq!(contract.operation_name, "profile");
                forklaunch_core::auth::typescript::TypeScriptRbacPolicy {
                    allowed_permissions: Some(vec!["read".into()]),
                    allowed_roles: Some(vec!["admin".into()]),
                    ..Default::default()
                }
                .authorize(&session.principal, Some(contract.access))
            });
        let router = app(config);
        for (authorization, status, count) in [
            ("missing", 401, 0),
            ("verified", 200, 1),
            ("verified-denied", 403, 2),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::get("/profile")
                        .header("authorization", authorization)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), status);
            assert_eq!(calls.load(Ordering::SeqCst), count);
            let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
            if status == 403 {
                assert!(
                    String::from_utf8_lossy(&bytes).contains("Invalid Authorization permissions.")
                );
            }
        }
    }

    #[cfg(not(feature = "cookie-session"))]
    #[tokio::test]
    async fn cookie_contract_fails_closed_without_adapter_feature() {
        let config = RouterConfig::default().with_authenticator(|_, _| async {
            panic!("cookie routes must not fall back to the bearer verifier")
        });
        let router = ForkLaunchRouter::anonymous()
            .get(
                RouteContract::get("Cookie", "Cookie", "/cookie")
                    .with_auth(forklaunch_core::contract::AuthConfig::cookie("session"))
                    .with_response(200, None),
                || async { "unexpected" },
            )
            .with_config(config)
            .into_router();
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/cookie")
                    .header("cookie", "session=unverified")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn auth_is_required_for_get_and_implicit_head_even_without_validation() {
        for method in ["GET", "HEAD"] {
            let response = app(RouterConfig::default())
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri("/profile")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
    }

    #[tokio::test]
    async fn rbac_runs_before_handlers_and_verified_claims_reach_typed_extractors() {
        let config = RouterConfig::default().with_authenticator(|request, _| async move {
            let credential = request
                .headers
                .get("authorization")
                .ok_or(AuthError::MissingCredentials)?;
            let role = match credential.as_str() {
                "Bearer admin" => "admin",
                "Bearer member" => "member",
                _ => return Err(AuthError::InvalidCredentials),
            };
            Ok(AuthenticatedSession::new(
                Principal {
                    roles: vec![role.to_owned()],
                    ..Principal::new("user-1")
                },
                serde_json::json!({"sub":"user-1","tenant":"tenant-1"}),
            ))
        });
        for (credential, expected) in [
            (None, 401),
            (Some("Bearer invalid"), 401),
            (Some("Bearer member"), 403),
            (Some("Bearer admin"), 200),
        ] {
            for method in ["GET", "HEAD"] {
                let mut request = Request::builder()
                    .method(method)
                    .uri("/profile")
                    .header("x-correlation-id", "auth-test");
                if let Some(credential) = credential {
                    request = request.header("authorization", credential);
                }
                let response = app(config.clone())
                    .oneshot(request.body(Body::empty()).unwrap())
                    .await
                    .unwrap();
                assert_eq!(response.status().as_u16(), expected);
                let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
                if method == "GET" && expected == 200 {
                    assert!(String::from_utf8_lossy(&body).contains("tenant-1"));
                }
                if method == "GET" && expected != 200 {
                    assert!(String::from_utf8_lossy(&body).contains("auth-test"));
                }
                if method == "HEAD" {
                    assert!(body.is_empty());
                }
            }
        }
    }

    #[tokio::test]
    async fn verified_session_reaches_local_handlers_and_public_routes_still_work() {
        let contract = RouteContract::get("Local", "Local", "/local")
            .with_access(AccessLevel::Authenticated)
            .with_response(200, None);
        let mut local = forklaunch_core::http::LocalRouter::new();
        local.register(contract.clone(), |request: FrameworkRequest| {
            assert_eq!(request.session.unwrap()["sub"], "user-1");
            Ok(forklaunch_core::http::FrameworkResponse::text(
                200, "verified",
            ))
        });
        let config = RouterConfig::default().with_authenticator(|_, _| async {
            Ok(AuthenticatedSession::new(
                Principal::new("user-1"),
                serde_json::json!({"sub":"user-1"}),
            ))
        });
        let router = ForkLaunchRouter::from_local_router(
            forklaunch_core::ServiceInfo::new("local", "1"),
            [contract],
            local,
        )
        .with_config(config)
        .into_router();
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/local")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let public = ForkLaunchRouter::anonymous()
            .get(
                RouteContract::get("Public", "Public", "/public").with_response(200, None),
                || async { "public" },
            )
            .into_router();
        assert_eq!(
            public
                .oneshot(
                    Request::builder()
                        .uri("/public")
                        .body(Body::empty())
                        .unwrap()
                )
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn rejected_jwt_status_matches_typescript_without_changing_opaque_sessions() {
        use forklaunch_core::contract::{AuthConfig, JwtAuthConfig};
        for auth in [
            None,
            Some(AuthConfig::bearer()),
            Some(AuthConfig::jwt(JwtAuthConfig::SignatureKey)),
        ] {
            for error in [
                AuthError::MissingCredentials,
                AuthError::InvalidCredentials,
                AuthError::Forbidden("insufficient role".into()),
            ] {
                let expected = if matches!(error, AuthError::Forbidden(_))
                    || (error == AuthError::InvalidCredentials
                        && matches!(auth, Some(AuthConfig::Jwt(_))))
                {
                    StatusCode::FORBIDDEN
                } else {
                    StatusCode::UNAUTHORIZED
                };
                let mut contract = RouteContract::get("Secure", "Secure", "/secure")
                    .with_access(AccessLevel::Authenticated)
                    .with_response(200, None);
                contract.auth = auth.clone();
                let config = RouterConfig::default().with_authenticator(move |_, _| {
                    let error = error.clone();
                    async move { Err(error) }
                });
                let router = ForkLaunchRouter::anonymous()
                    .with_config(config)
                    .get(contract, || async { StatusCode::OK })
                    .into_router();
                let response = router
                    .oneshot(
                        Request::builder()
                            .uri("/secure")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), expected);
                let body = to_bytes(response.into_body(), 65536).await.unwrap();
                let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
                assert!(body["correlationId"].is_string());
            }
        }
    }

    #[tokio::test]
    async fn signed_jwt_verifies_before_grants_are_accepted() {
        let config = RouterConfig::default().with_authenticator(|request, _| async move {
            let token = request
                .headers
                .get("authorization")
                .and_then(|header| parse_bearer(header))
                .ok_or(AuthError::MissingCredentials)?;
            let principal = verify_jwt_hs256(token, b"secret")?;
            let claims =
                serde_json::to_value(&principal).map_err(|_| AuthError::InvalidCredentials)?;
            Ok(AuthenticatedSession::new(principal, claims))
        });
        // A syntactically JWT-shaped token with forged admin claims must be rejected.
        let response = app(config).oneshot(Request::builder().uri("/profile").header("authorization", "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhdHRhY2tlciIsInJvbGVzIjpbImFkbWluIl0sImV4cCI6NDEwMjQ0NDgwMH0.invalid").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}

#[cfg(all(test, feature = "adapter"))]
mod telemetry_tests {
    use crate::{ForkLaunchRouter, RouterConfig};
    use axum::{body::Body, http::Request};
    use forklaunch_core::contract::{AccessLevel, RouteContract};
    use forklaunch_core::telemetry::OpenTelemetryCollector;
    use opentelemetry::{metrics::MeterProvider, trace::TracerProvider};
    use opentelemetry_sdk::{
        metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider},
        trace::{InMemorySpanExporter, SdkTracerProvider},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn http_middleware_exports_success_auth_and_unmatched_request_telemetry() {
        let metrics = InMemoryMetricExporter::default();
        let meters = SdkMeterProvider::builder()
            .with_reader(PeriodicReader::builder(metrics.clone()).build())
            .build();
        let spans = InMemorySpanExporter::default();
        let traces = SdkTracerProvider::builder()
            .with_simple_exporter(spans.clone())
            .build();
        let config = RouterConfig {
            telemetry: Some(
                OpenTelemetryCollector::with_meter("test", meters.meter("test"))
                    .with_tracer(traces.tracer("test")),
            ),
            ..RouterConfig::default()
        };
        let app = ForkLaunchRouter::anonymous()
            .get(
                RouteContract::get("User", "User", "/users/{id}").with_response(200, None),
                || async { "ok" },
            )
            .get(
                RouteContract::get("Private", "Private", "/private")
                    .with_access(AccessLevel::Authenticated)
                    .with_response(200, None),
                || async { "private" },
            )
            .with_config(config)
            .into_router();
        for (path, status) in [
            ("/users/secret-user-id", 200),
            ("/private", 401),
            ("/unknown", 404),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .header("x-correlation-id", "http-correlation")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), status);
            axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap();
        }
        meters.force_flush().unwrap();
        traces.force_flush().unwrap();
        let exported = metrics.get_finished_metrics().unwrap();
        let names: Vec<_> = exported
            .iter()
            .flat_map(opentelemetry_sdk::metrics::data::ResourceMetrics::scope_metrics)
            .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics)
            .map(opentelemetry_sdk::metrics::data::Metric::name)
            .collect();
        assert!(names.contains(&"forklaunch.auth.failures"));
        assert!(names.contains(&"forklaunch.http.errors"));
        assert!(names.contains(&"http.server.request.count"));
        let exported = spans.get_finished_spans().unwrap();
        assert_eq!(exported.len(), 3);
        assert!(exported.iter().any(|span| span.name == "GET /users/{id}"));
        assert!(
            !exported
                .iter()
                .any(|span| span.name.contains("secret-user-id"))
        );
        assert!(exported.iter().all(|span| span.attributes.contains(
            &opentelemetry::KeyValue::new("forklaunch.correlation_id", "http-correlation")
        )));
        meters.shutdown().unwrap();
        traces.shutdown().unwrap();
    }
}
