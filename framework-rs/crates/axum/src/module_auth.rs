//! Shared application-owned HMAC and asymmetric JWKS authentication policy.
mod telemetry;
use crate::session::AuthenticatedSession;
use forklaunch_core::auth::{
    AuthError, JwtClaims, Principal,
    jwt::RemoteJwks,
    parse_bearer,
    request_hmac::{HmacAuthorization, SignedBody},
};
use forklaunch_core::contract::{AuthConfig, RouteContract};
use forklaunch_core::http::{Body, FrameworkRequest};
use jsonwebtoken::{Algorithm, Validation};
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
type RolesFuture = Pin<Box<dyn Future<Output = Result<Vec<String>, AuthError>> + Send>>;
type RolesResolver = Arc<dyn Fn(serde_json::Value) -> RolesFuture + Send + Sync>;

/// Trusted server-owned keys and JWT policy. Secret bytes are never formatted.
/// Default roles come only from verified issuer claims. Configure a resolver to
/// surface roles from a trusted IAM service rather than installing a role stub.
pub struct HmacJwksAuth {
    secret: Vec<u8>,
    jwks: RemoteJwks,
    validation: Validation,
    roles: RolesResolver,
    max_skew: Duration,
    route_hmac_keys: BTreeMap<(String, String), BTreeMap<String, Vec<u8>>>,
}

/// Safe result categories for trusted application-owned HMAC payload profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HmacPayloadError {
    NotHmac,
    UnknownKey,
    InvalidSignature,
}

impl HmacJwksAuth {
    /// # Errors
    /// Requires a nonempty HMAC secret, asymmetric JWT algorithms and expiry
    /// validation. Endpoint/cache policy is owned by the supplied JWKS verifier.
    pub fn new(
        secret: Vec<u8>,
        jwks: RemoteJwks,
        mut validation: Validation,
    ) -> Result<Self, AuthError> {
        if secret.is_empty()
            || !validation.validate_exp
            || validation.algorithms.is_empty()
            || validation.algorithms.iter().any(|algorithm| {
                matches!(
                    algorithm,
                    Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512
                )
            })
        {
            return Err(AuthError::InvalidCredentials);
        }
        validation.required_spec_claims.insert("exp".to_owned());
        Ok(Self {
            secret,
            jwks,
            validation,
            roles: Arc::new(|claims| {
                Box::pin(async move {
                    serde_json::from_value::<JwtClaims>(claims)
                        .map(|claims| claims.roles)
                        .map_err(|_| AuthError::InvalidCredentials)
                })
            }),
            max_skew: Duration::from_secs(300),
            route_hmac_keys: BTreeMap::new(),
        })
    }

    /// Replace the default HMAC key policy for one declared method and path.
    /// Other routes retain their existing policy. Keys stay server-owned and
    /// never enter exported contracts or SDK metadata.
    /// # Errors
    /// Rejects non-HMAC routes, empty/duplicate key IDs or empty secrets.
    pub fn with_hmac_keys_for_route(
        mut self,
        route: &RouteContract,
        keys: impl IntoIterator<Item = (String, Vec<u8>)>,
    ) -> Result<Self, AuthError> {
        if !matches!(route.auth, Some(AuthConfig::Hmac(_))) {
            return Err(AuthError::InvalidCredentials);
        }
        let mut policy = BTreeMap::new();
        for (id, secret) in keys {
            if id.is_empty() || secret.is_empty() || policy.insert(id, secret).is_some() {
                return Err(AuthError::InvalidCredentials);
            }
        }
        if policy.is_empty() {
            return Err(AuthError::InvalidCredentials);
        }
        self.route_hmac_keys
            .insert((route.method.as_str().into(), route.path.clone()), policy);
        Ok(self)
    }

    /// Verify an application-owned canonical payload with this route's immutable
    /// server key policy. Keys are never returned. The caller owns parsing, wire
    /// canonicalization and freshness/replay policy; the ordinary `authenticate`
    /// method retains its existing strict header and five-minute freshness checks.
    /// # Errors
    /// Distinguishes absent server keys from invalid signatures without exposing
    /// key material. A service principal is returned only after MAC verification.
    pub fn verify_hmac_payload_for_route(
        &self,
        route: &RouteContract,
        key_id: &str,
        payload: &[u8],
        signature: &str,
    ) -> Result<AuthenticatedSession, HmacPayloadError> {
        if !matches!(route.auth, Some(AuthConfig::Hmac(_))) {
            return Err(HmacPayloadError::NotHmac);
        }
        let secret = if let Some(policy) = self
            .route_hmac_keys
            .get(&(route.method.as_str().into(), route.path.clone()))
        {
            policy.get(key_id).ok_or(HmacPayloadError::UnknownKey)?
        } else if key_id == "default" {
            &self.secret
        } else {
            return Err(HmacPayloadError::UnknownKey);
        };
        if !forklaunch_core::auth::hmac_verify(secret, payload, signature) {
            return Err(HmacPayloadError::InvalidSignature);
        }
        let subject = format!("service:{key_id}");
        Ok(AuthenticatedSession::new(
            Principal::new(&subject),
            serde_json::json!({"sub":subject}),
        ))
    }

    #[must_use]
    pub fn with_role_resolver<F, R>(mut self, resolver: F) -> Self
    where
        F: Fn(serde_json::Value) -> R + Send + Sync + 'static,
        R: Future<Output = Result<Vec<String>, AuthError>> + Send + 'static,
    {
        self.roles = Arc::new(move |claims| Box::pin(resolver(claims)));
        self
    }

    /// Verifies the credential declared by the selected route.
    /// # Errors
    /// Returns a sanitized credential/policy error.
    pub async fn authenticate(
        &self,
        request: FrameworkRequest,
        contract: RouteContract,
    ) -> Result<AuthenticatedSession, AuthError> {
        let scheme = if matches!(contract.auth, Some(AuthConfig::Hmac(_))) {
            "hmac"
        } else {
            "jwt"
        };
        telemetry::observe(scheme, self.authenticate_inner(request, contract)).await
    }

    async fn authenticate_inner(
        &self,
        request: FrameworkRequest,
        contract: RouteContract,
    ) -> Result<AuthenticatedSession, AuthError> {
        let header = request
            .headers
            .get("authorization")
            .ok_or(AuthError::MissingCredentials)?;
        if matches!(contract.auth, Some(AuthConfig::Hmac(_))) {
            let authorization = HmacAuthorization::parse(header)?;
            let body = match &request.body {
                Body::Empty => SignedBody::Absent,
                Body::Text(text) => SignedBody::Text(text),
                Body::Binary(bytes) => SignedBody::Bytes(bytes),
            };
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| AuthError::InvalidCredentials)?;
            let secret = if let Some(policy) = self
                .route_hmac_keys
                .get(&(contract.method.as_str().into(), contract.path.clone()))
            {
                policy
                    .get(authorization.key_id())
                    .ok_or(AuthError::InvalidCredentials)?
            } else if authorization.key_id() == "default" {
                &self.secret
            } else {
                return Err(AuthError::InvalidCredentials);
            };
            if !authorization.verify_at(
                secret,
                request.method.as_str(),
                &request.path,
                body,
                i64::try_from(now.as_millis()).map_err(|_| AuthError::InvalidCredentials)?,
                self.max_skew,
            ) {
                return Err(AuthError::InvalidCredentials);
            }
            let subject = format!("service:{}", authorization.key_id());
            return Ok(AuthenticatedSession::new(
                Principal::new(&subject),
                serde_json::json!({"sub":subject}),
            ));
        }
        let token = parse_bearer(header)
            .filter(|token| token.len() <= 8192)
            .ok_or(AuthError::InvalidCredentials)?;
        let claims: serde_json::Value = self.jwks.verify(token, &self.validation).await?;
        let typed: JwtClaims =
            serde_json::from_value(claims.clone()).map_err(|_| AuthError::InvalidCredentials)?;
        let mut principal = Principal::from_claims(typed);
        principal.roles = (self.roles)(claims.clone()).await?;
        Ok(AuthenticatedSession::new(principal, claims))
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod head_auth_tests;

#[cfg(test)]
mod route_keys;
