//! Bearer-JWT sessions verified against a remote JWKS: the policy plain native
//! services use to accept IAM-issued session tokens. Enabled by `native-jwt`
//! (also part of `agent`).
use super::SessionPolicy;
use crate::session::{AuthenticatedSession, AuthenticationFuture};
use forklaunch_core::auth::jwt::{Algorithm, RemoteJwks, RemoteJwksConfig, Validation};
use forklaunch_core::auth::{AuthError, JwtClaims, Principal, parse_bearer};
use forklaunch_core::config;
use forklaunch_core::contract::RouteContract;
use forklaunch_core::http::FrameworkRequest;
use std::sync::Arc;

/// Bearer-JWT session policy over a remote JWKS: the token's verified claims
/// become the session (`sub` is the agent's user id). Only asymmetric
/// algorithms are accepted.
#[must_use]
pub fn jwt_policy(jwks: RemoteJwks, validation: Validation) -> SessionPolicy {
    let verifier = Arc::new((jwks, validation));
    SessionPolicy::new(
        Arc::new(move |request: FrameworkRequest, _: RouteContract| {
            let verifier = verifier.clone();
            Box::pin(async move {
                let header = request
                    .headers
                    .get("authorization")
                    .ok_or(AuthError::MissingCredentials)?;
                let token = parse_bearer(header)
                    .filter(|token| token.len() <= 8192)
                    .ok_or(AuthError::InvalidCredentials)?;
                let (jwks, validation) = &*verifier;
                let claims: serde_json::Value = jwks.verify(token, validation).await?;
                let typed: JwtClaims = serde_json::from_value(claims.clone())
                    .map_err(|_| AuthError::InvalidCredentials)?;
                Ok(AuthenticatedSession::new(
                    Principal::from_claims(typed),
                    claims,
                ))
            }) as AuthenticationFuture
        }),
        Arc::new(|_, _| Ok(())),
    )
}

/// [`jwt_policy`] from the service settings: `JWKS_PUBLIC_KEY_URL` (required),
/// optional `JWT_ISSUER` and `JWT_AUDIENCE` (each unenforced when unset), with
/// the asymmetric algorithms the framework's other services accept.
/// # Errors
/// Names a missing JWKS URL and rejects an invalid one.
pub fn jwt_policy_from_env() -> Result<SessionPolicy, Box<dyn std::error::Error + Send + Sync>> {
    let url: String = config::required("JWKS_PUBLIC_KEY_URL")?;
    let jwks = RemoteJwks::new(RemoteJwksConfig::new(url))
        .map_err(|_| "JWKS_PUBLIC_KEY_URL must be a valid http(s) URL")?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.algorithms = vec![
        Algorithm::RS256,
        Algorithm::RS384,
        Algorithm::RS512,
        Algorithm::PS256,
        Algorithm::PS384,
        Algorithm::PS512,
        Algorithm::ES256,
        Algorithm::ES384,
        Algorithm::EdDSA,
    ];
    validation.validate_nbf = true;
    validation.leeway = 0;
    validation.validate_aud = false;
    if let Some(issuer) = config::optional::<String>("JWT_ISSUER")? {
        validation.set_issuer(&[issuer]);
    }
    if let Some(audience) = config::optional::<String>("JWT_AUDIENCE")? {
        validation.validate_aud = true;
        validation.set_audience(&[audience]);
    }
    Ok(jwt_policy(jwks, validation))
}

/// [`jwt_policy_from_env`] when `JWKS_PUBLIC_KEY_URL` is set; otherwise a
/// policy that rejects every protected request (401), so a service without an
/// identity provider still starts and its public routes still work.
/// # Errors
/// Rejects an invalid JWKS URL or issuer/audience setting.
pub fn optional_jwt_policy_from_env()
-> Result<SessionPolicy, Box<dyn std::error::Error + Send + Sync>> {
    if config::optional::<String>("JWKS_PUBLIC_KEY_URL")?.is_some() {
        return jwt_policy_from_env();
    }
    Ok(SessionPolicy::new(
        Arc::new(|_: FrameworkRequest, _: RouteContract| {
            Box::pin(async { Err(AuthError::MissingCredentials) }) as AuthenticationFuture
        }),
        Arc::new(|_, _| Ok(())),
    ))
}
