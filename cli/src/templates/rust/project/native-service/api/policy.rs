//! Endpoint admission for `#[access(Protected)]` operations: the caller's IAM
//! session JWT (`Authorization: Bearer`) is verified against the JWKS at
//! `JWKS_PUBLIC_KEY_URL`, honouring `JWT_ISSUER` and `JWT_AUDIENCE`.
//!
//! Bind a protected operation with `Operation::bind_authorized(UserPolicy, handler)`;
//! the handler receives the verified `AuthenticatedSession` (subject, roles,
//! permissions and claims such as `tenant_id`) to authorize per resource.
//! `#[access(Internal)]` (HMAC) operations need a policy implementing
//! `OriginalBodySignedPolicy` instead; see that trait's documentation.
use crate::registrations::AppState;
use axum::http::request::Parts;
use forklaunch_axum::native::{EndpointError, EndpointPolicy, PolicyCapabilities, RouteContract};
use forklaunch_axum::session::AuthenticatedSession;
use forklaunch_core::contract::bindings::AuthorizationPolicy;

/// Verifies the IAM session JWT with the policy held in `AppState::auth`.
#[derive(Clone)]
pub struct UserPolicy;

impl AuthorizationPolicy for UserPolicy {}

impl EndpointPolicy<AppState> for UserPolicy {
    const CAPABILITIES: PolicyCapabilities = PolicyCapabilities::Authorized;
    type Context = AuthenticatedSession;

    async fn authorize(
        &self,
        parts: &mut Parts,
        state: &AppState,
        route: &RouteContract,
    ) -> Result<AuthenticatedSession, EndpointError> {
        state.auth.authorize(parts, state, route).await
    }
}
