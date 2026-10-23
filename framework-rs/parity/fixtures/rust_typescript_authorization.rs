use axum::{Extension, Json};
use forklaunch_core::auth::{
    AuthError, parse_bearer,
    typescript::{
        TypeScriptRbacPolicy,
        basic::{verify_basic, verify_basic_resource},
        hmac::verify_hmac,
        jwt::verify_jwt_resource,
        session::TypeScriptSessionResolver,
    },
    verify_jwt_hs256,
};
use forklaunch_axum::{ForkLaunchRouter, RouterConfig, session::AuthenticatedSession};
pub use contracts::policy_fixture::contracts;
use forklaunch_core::http::{ErrorKind, ErrorResponse};
use forklaunch_validator::{ParseError, pretty_print_parse_errors};
use serde_json::{Value, json};
const SECRET: &[u8] = b"fixture-only-session-authorization-key";
pub fn register(mut router: ForkLaunchRouter) -> ForkLaunchRouter {
    let config = RouterConfig {
        auth_error_format: if std::env::var("FORKLAUNCH_TEST_AUTH_TEXT").as_deref() == Ok("1") {
            forklaunch_axum::AuthErrorFormat::TypeScriptText
        } else {
            forklaunch_axum::AuthErrorFormat::Json
        },
        // The generated native application serves the docs and OpenAPI.
        openapi_enabled: false,
        docs_enabled: false,
        ..RouterConfig::default()
    }
        .with_response_authenticator(|request, contract| async move {
            if contract.operation_name == "checkHmac" {
                use forklaunch_core::auth::request_hmac::{HmacAuthorization, SignedBody};
                let keys = std::collections::BTreeMap::from([("peer".to_owned(), "fixture-hmac-secret".to_owned())]);
                verify_hmac(request.headers.get("authorization").map(String::as_str), None, Some(&keys), |credential| async move {
                    let Some(secret) = credential.secret_key else { return Ok(false); };
                    let wire = format!("HMAC keyId={} ts={} nonce={} signature={}", credential.key_id, credential.timestamp, credential.nonce, credential.signature);
                    let Ok(parsed) = HmacAuthorization::parse(&wire) else { return Ok(false); };
                    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                        .map_err(|_| ErrorResponse::new(ErrorKind::Internal, "clock unavailable"))?;
                    let body = match &request.body {
                        forklaunch_core::http::Body::Empty => SignedBody::Absent,
                        forklaunch_core::http::Body::Text(text) => SignedBody::Text(text),
                        forklaunch_core::http::Body::Binary(bytes) => SignedBody::Bytes(bytes),
                    };
                    Ok(parsed.verify_at(secret.as_bytes(), request.method.as_str(), &request.path, body,
                        i64::try_from(now.as_millis()).unwrap_or(i64::MAX), std::time::Duration::from_secs(60)))
                }).await?;
                return Ok(AuthenticatedSession::new(forklaunch_core::auth::Principal::new("trusted-peer"), Value::Null));
            }
            if contract.operation_name == "checkJwtResource" {
                let claims = verify_jwt_resource(
                    request.headers.get("authorization").map(String::as_str),
                    None,
                    |token| async move {
                        tokio::task::yield_now().await;
                        let principal = verify_jwt_hs256(token, SECRET).map_err(|_| {
                            ErrorResponse::new(ErrorKind::Internal, "private verification details")
                        })?;
                        Ok(json!({"sub":principal.subject, "roles":principal.roles, "team":"jwt-verified-team"}))
                    },
                ).await?;
                let subject = claims.get("sub").and_then(Value::as_str)
                    .ok_or_else(|| ErrorResponse::new(ErrorKind::Validation, "invalid session"))?;
                let mut principal = forklaunch_core::auth::Principal::new(subject);
                principal.roles = serde_json::from_value(claims["roles"].clone())
                    .map_err(|_| ErrorResponse::new(ErrorKind::Validation, "invalid session"))?;
                return Ok(AuthenticatedSession::new(principal, claims));
            }
            if contract.operation_name == "checkResource" {
                let claims = verify_basic_resource(
                    request.headers.get("authorization").map(String::as_str),
                    None,
                    |token| async move {
                        tokio::task::yield_now().await;
                        Ok(match token {
                            "owned-opaque-token" => json!({"sub":"resource-user", "role":"admin", "team":"verified-team"}),
                            "owned-reader-token" => json!({"sub":"resource-user", "role":"reader", "team":"verified-team"}),
                            "owned-invalid-session" => json!({"sub":42}),
                            _ => Value::Null,
                        })
                    },
                ).await?;
                let subject = claims.get("sub").and_then(Value::as_str)
                    .ok_or_else(|| ErrorResponse::new(ErrorKind::Validation, "invalid session"))?;
                let mut principal = forklaunch_core::auth::Principal::new(subject);
                principal.roles = claims.get("role").and_then(Value::as_str).map(|role| vec![role.into()]).unwrap_or_default();
                return Ok(AuthenticatedSession::new(principal, claims));
            }
            if contract.operation_name == "checkBasic" {
                let principal = verify_basic(
                    request.headers.get("authorization").map(String::as_str),
                    None,
                    |user, password| user == "fixture-user" && password == "fixture-password",
                )?;
                return Ok(AuthenticatedSession::new(principal, json!({})));
            }
            let invalid = || ErrorResponse::new(ErrorKind::Forbidden, "invalid credentials");
            let header = request
                .headers
                .get("authorization")
                .ok_or_else(|| AuthError::MissingCredentials.to_error_response())?;
            let token = parse_bearer(header).ok_or_else(invalid)?;
            let principal = verify_jwt_hs256(token, SECRET).map_err(|_| invalid())?;
            if matches!(contract.operation_name.as_str(), "checkSession" | "checkMissingSessionCallback") {
                let claims = json!({"sub":principal.subject,"team":"ordered-session-team"});
                let resolver = SessionResolver {
                    principal: &principal,
                    missing_permissions: contract.operation_name == "checkMissingSessionCallback",
                };
                TypeScriptRbacPolicy {
                    required_scope: Some("team".into()),
                    scope_hierarchy: Some(vec!["team".into()]),
                    allowed_permissions: Some(vec!["read".into()]),
                    allowed_roles: Some(vec!["admin".into()]),
                    require_active_subscription: true,
                    required_features: vec!["reports".into()],
                    ..Default::default()
                }.authorize_session(&claims, Some(contract.access), &resolver).await?;
                return Ok(AuthenticatedSession::new(principal, claims));
            }
            Ok(AuthenticatedSession::new(principal, json!({})))
        })
        .with_authorizer(|session, contract| {
            if contract.operation_name == "checkHmac" { return Ok(()); }
            // These two routes perform their complete async policy in the
            // authenticator above, before an authenticated session is returned.
            if matches!(contract.operation_name.as_str(), "checkSession" | "checkMissingSessionCallback") { return Ok(()); }
            let policy = match contract.operation_name.as_str() {
                "checkResource" | "checkJwtResource" => TypeScriptRbacPolicy {
                    allowed_roles: Some(vec!["admin".into()]),
                    ..Default::default()
                },
                "checkPermissions" => TypeScriptRbacPolicy {
                    allowed_permissions: Some(vec!["read".into()]),
                    allowed_roles: Some(vec!["admin".into()]),
                    ..Default::default()
                },
                "checkScope" => TypeScriptRbacPolicy {
                    required_scope: Some("team".into()),
                    scope_hierarchy: Some(vec!["admin".into(), "team".into()]),
                    ..Default::default()
                },
                "checkFeatures" => TypeScriptRbacPolicy {
                    require_active_subscription: true,
                    required_features: vec!["reports".into()],
                    ..Default::default()
                },
                _ => TypeScriptRbacPolicy::default(),
            };
            policy.authorize(&session.principal, Some(contract.access))
        });
    router = router.with_config(config);
    for contract in contracts() {
        router = router.get(
            contract,
            |Extension(session): Extension<AuthenticatedSession>| async move {
                if let Some(team) = session.claims.get("team") {
                    return Json::<Value>(
                        json!({"subject":session.principal.subject, "team":team}),
                    );
                }
                Json::<Value>(json!({"subject":session.principal.subject}))
            },
        );
    }
    router
}

struct SessionResolver<'a> {
    principal: &'a forklaunch_core::auth::Principal,
    missing_permissions: bool,
}
impl TypeScriptSessionResolver for SessionResolver<'_> {
    fn validate_session(&self, claims: &Value) -> Result<(), ErrorResponse> {
        if claims["sub"] == "invalid-session" {
            let errors = [
                ParseError { path: vec!["sub".into()], message: "subject rejected".into() },
                ParseError { path: vec![], message: "session rejected".into() },
            ];
            return Err(ErrorResponse::new(ErrorKind::Validation, format!(
                "Invalid session: {}", pretty_print_parse_errors(Some(&errors), Some("Session"))
            )));
        }
        Ok(())
    }
    async fn scopes(&self, _: &Value) -> Result<Option<Vec<String>>, ErrorResponse> {
        tokio::task::yield_now().await;
        Ok(Some(self.principal.scopes.clone()))
    }
    async fn permissions(&self, _: &Value) -> Result<Option<Vec<String>>, ErrorResponse> {
        tokio::task::yield_now().await;
        Ok((!self.missing_permissions).then(|| self.principal.permissions.clone()))
    }
    async fn roles(&self, _: &Value) -> Result<Option<Vec<String>>, ErrorResponse> {
        panic!("permission rules take precedence over role callbacks")
    }
    async fn subscription(&self, _: &Value) -> Result<Option<bool>, ErrorResponse> {
        tokio::task::yield_now().await;
        Ok(Some(self.principal.active_subscription))
    }
    async fn features(&self, _: &Value) -> Result<Option<Vec<String>>, ErrorResponse> {
        tokio::task::yield_now().await;
        Ok(Some(self.principal.features.clone()))
    }
}
