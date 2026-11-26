use super::*;
use crate::native::ContractRouter;
use axum::{body::Body, http::Request};
use forklaunch_core::{
    auth::{JwtClaims, issue_jwt_hs256, parse_bearer, verify_jwt_hs256},
    contract::{
        HttpMethod,
        endpoint::{Endpoint, NoBody, TextBody},
    },
};
use tower::ServiceExt;
const KEY: &[u8] = b"test-only-managed-native-jwt-key";
#[forklaunch_core::contract::api]
mod operations {
    use super::*;
    #[access(Protected)]
    pub fn profile() -> Endpoint<NoBody, TextBody> {
        Endpoint::new("profile", "profile", HttpMethod::Get, "/profile", 200).unwrap()
    }
}
fn policy() -> SessionPolicy {
    let authenticate: Authenticator = Arc::new(|request, _| {
        Box::pin(async move {
            let token = request
                .headers
                .get("authorization")
                .and_then(|value| parse_bearer(value))
                .ok_or(AuthError::MissingCredentials)?;
            let principal = verify_jwt_hs256(token, KEY)?;
            Ok(AuthenticatedSession::new(
                principal,
                serde_json::json!({"verified":true}),
            ))
        })
    });
    SessionPolicy::with_typescript_rules(
        authenticate,
        TypeScriptRbacPolicy {
            allowed_permissions: Some(vec!["read".into()]),
            allowed_roles: Some(vec!["ignored-by-permission-precedence".into()]),
            required_scope: Some("notes".into()),
            scope_hierarchy: Some(vec!["notes".into()]),
            required_features: vec!["notes".into()],
            require_active_subscription: true,
            ..Default::default()
        },
    )
}
fn claims() -> JwtClaims {
    JwtClaims {
        sub: "verified-user".into(),
        exp: usize::MAX / 2,
        roles: vec![],
        permissions: vec!["read".into()],
        scopes: vec!["notes".into()],
        features: vec!["notes".into()],
        active_subscription: true,
    }
}
#[tokio::test]
async fn actual_signed_jwt_and_shared_authorization_gate_native_static_handler() {
    let app = operations::Bindings {
        profile: operations::Profile::bind_authorized(
            policy(),
            |_, session: AuthenticatedSession, ()| async move {
                assert_eq!(session.claims["verified"], true);
                Ok(session.principal.subject)
            },
        ),
    }
    .mount(ContractRouter::<()>::new(operations::registry()))
    .unwrap()
    .finish()
    .unwrap();
    let base = claims();
    let mut no_permission = base.clone();
    no_permission.permissions.clear();
    let mut no_scope = base.clone();
    no_scope.scopes.clear();
    let mut no_feature = base.clone();
    no_feature.features.clear();
    let mut inactive = base.clone();
    inactive.active_subscription = false;
    let mut expired = base.clone();
    expired.exp = 1;
    for (claims, key, status) in [
        (base, KEY, 200),
        (no_permission, KEY, 403),
        (no_scope, KEY, 403),
        (no_feature, KEY, 403),
        (inactive, KEY, 403),
        (expired, KEY, 401),
        (claims(), b"wrong-key".as_slice(), 401),
    ] {
        let token = issue_jwt_hs256(&claims, key).unwrap();
        let response = app
            .clone()
            .oneshot(
                Request::get("/profile")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
    }
    let response = app
        .oneshot(Request::get("/profile").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
}
#[test]
fn session_policy_never_claims_signed_body_or_replay_capability() {
    let endpoint = operations::profile().with_access(AccessLevel::Internal);
    let mut registry = forklaunch_core::contract::ContractRegistry::new();
    registry.register(endpoint.contract());
    assert!(
        ContractRouter::<()>::new(registry)
            .bind_authorized(endpoint, policy(), |_, _, ()| async { Ok("never".into()) })
            .is_err()
    );
}

#[tokio::test]
async fn verified_session_roles_and_duplicate_credentials_are_enforced() {
    let authenticate: Authenticator = Arc::new(|request, _| {
        Box::pin(async move {
            let roles = match request.headers.get("cookie").map(String::as_str) {
                Some("session=administrator") => vec!["admin".into()],
                Some("session=reader") => vec!["reader".into()],
                _ => return Err(AuthError::InvalidCredentials),
            };
            let mut principal = forklaunch_core::auth::Principal::new("server-resolved");
            principal.roles = roles;
            Ok(AuthenticatedSession::new(
                principal,
                serde_json::json!({"id":"server-resolved"}),
            ))
        })
    });
    let policy = SessionPolicy::with_typescript_rules(
        authenticate,
        TypeScriptRbacPolicy {
            allowed_roles: Some(vec!["admin".into()]),
            ..Default::default()
        },
    );
    let app = operations::Bindings {
        profile: operations::Profile::bind_authorized(
            policy,
            |_, session: AuthenticatedSession, ()| async move { Ok(session.principal.subject) },
        ),
    }
    .mount(ContractRouter::<()>::new(operations::registry()))
    .unwrap()
    .finish()
    .unwrap();
    for (cookie, status) in [
        ("session=administrator", 200),
        ("session=reader", 403),
        ("role=admin", 401),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::get("/profile")
                    .header("cookie", cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
    }
    let response = app
        .oneshot(
            Request::get("/profile")
                .header("cookie", "session=administrator")
                .header("cookie", "session=reader")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
}
