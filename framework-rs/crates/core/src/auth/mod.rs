#![forbid(unsafe_code)]

//! Authentication and authorization primitives for the Rust framework.
//!
//! Covers the credential mechanisms `ForkLaunch` services rely on — HMAC, JWT
//! (HS256 signature key), and Basic — together with the RBAC model
//! (roles, permissions, scopes with hierarchy, feature entitlements, and
//! subscription gating). All failures surface as [`AuthError`], which converts
//! into the framework's standard [`ErrorResponse`].

use crate::http::{ErrorKind, ErrorResponse};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use forklaunch_core::contract::RbacConfig;
use hmac::{Hmac, KeyInit, Mac};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

pub mod jwt;

#[cfg(feature = "session-client")]
pub mod session_client;

#[cfg(feature = "oidc")]
pub mod oidc;

/// Explicit compatibility policy for verified TypeScript-style sessions.
pub mod typescript;

/// TypeScript-compatible service-to-service request signatures.
pub mod request_hmac;

type HmacSha256 = Hmac<Sha256>;

/// An authenticated caller, enriched from a verified credential.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    /// Subject identifier (e.g. user or service id).
    pub subject: String,
    /// Granted roles.
    pub roles: Vec<String>,
    /// Granted fine-grained permissions.
    pub permissions: Vec<String>,
    /// Granted scopes.
    pub scopes: Vec<String>,
    /// Enabled feature entitlements.
    pub features: Vec<String>,
    /// Whether the caller has an active subscription.
    pub active_subscription: bool,
}

impl Principal {
    /// Creates a principal for the given subject with no grants.
    #[must_use]
    pub fn new(subject: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
            ..Self::default()
        }
    }

    /// Enriches an authenticated principal from verified claims.
    #[must_use]
    pub fn from_claims(claims: JwtClaims) -> Self {
        Self {
            subject: claims.sub,
            roles: claims.roles,
            permissions: claims.permissions,
            scopes: claims.scopes,
            features: claims.features,
            active_subscription: claims.active_subscription,
        }
    }
}

/// Reasons authentication or authorization can fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// No credential was presented.
    MissingCredentials,
    /// A credential was presented but could not be verified.
    InvalidCredentials,
    /// The caller authenticated but is not permitted to access the resource.
    Forbidden(String),
}

impl AuthError {
    /// Converts the failure into a safe framework error response.
    #[must_use]
    pub fn to_error_response(&self) -> ErrorResponse {
        match self {
            Self::MissingCredentials => {
                ErrorResponse::new(ErrorKind::Unauthorized, "missing credentials")
            }
            Self::InvalidCredentials => {
                ErrorResponse::new(ErrorKind::Unauthorized, "invalid credentials")
            }
            Self::Forbidden(reason) => ErrorResponse::new(ErrorKind::Forbidden, reason.clone()),
        }
    }
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingCredentials => write!(formatter, "missing credentials"),
            Self::InvalidCredentials => write!(formatter, "invalid credentials"),
            Self::Forbidden(reason) => write!(formatter, "forbidden: {reason}"),
        }
    }
}

impl std::error::Error for AuthError {}

/// JWT claims understood by the framework, including RBAC grants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    /// Subject identifier.
    pub sub: String,
    /// Expiry as a UNIX timestamp (seconds).
    pub exp: usize,
    /// Granted roles.
    #[serde(default)]
    pub roles: Vec<String>,
    /// Granted permissions.
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Granted scopes.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Enabled feature entitlements.
    #[serde(default)]
    pub features: Vec<String>,
    /// Whether the caller has an active subscription.
    #[serde(default)]
    pub active_subscription: bool,
}

/// Extracts a bearer token from an `Authorization` header value.
#[must_use]
pub fn parse_bearer(header: &str) -> Option<&str> {
    header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

/// Parses a `Basic` `Authorization` header value into `(username, password)`.
#[must_use]
pub fn parse_basic(header: &str) -> Option<(String, String)> {
    let encoded = header
        .strip_prefix("Basic ")
        .or_else(|| header.strip_prefix("basic "))?;
    let decoded = STANDARD.decode(encoded.trim()).ok()?;
    let text = String::from_utf8(decoded).ok()?;
    let (username, password) = text.split_once(':')?;
    Some((username.to_owned(), password.to_owned()))
}

/// Verifies a HS256 JWT against a shared signature key and returns a principal.
///
/// # Errors
///
/// Returns [`AuthError::InvalidCredentials`] when the token is malformed,
/// expired, or signed with a different key.
pub fn verify_jwt_hs256(token: &str, signature_key: &[u8]) -> Result<Principal, AuthError> {
    if signature_key.is_empty() {
        return Err(AuthError::InvalidCredentials);
    }
    jwt::check_header_policy(token)?;
    let mut validation = Validation::new(Algorithm::HS256);
    validation.leeway = 0;
    validation.validate_nbf = true;
    let data =
        decode::<serde_json::Value>(token, &DecodingKey::from_secret(signature_key), &validation)
            .map_err(|_| AuthError::InvalidCredentials)?;
    let now = jsonwebtoken::get_current_timestamp();
    let now_number = serde_json::Number::from(now)
        .as_f64()
        .ok_or(AuthError::InvalidCredentials)?;
    // JOSE validates optional NumericDate types even without maxTokenAge.
    // Keep fractional nbf values rather than silently truncating them.
    for name in ["nbf", "iat"] {
        if let Some(value) = data.claims.get(name) {
            let numeric_date = value.as_f64().ok_or(AuthError::InvalidCredentials)?;
            if name == "nbf" && numeric_date > now_number {
                return Err(AuthError::InvalidCredentials);
            }
        }
    }
    let claims: JwtClaims =
        serde_json::from_value(data.claims).map_err(|_| AuthError::InvalidCredentials)?;
    // jsonwebtoken's comparison is strict '<'; JOSE expires at exp <= now.
    if u64::try_from(claims.exp).map_err(|_| AuthError::InvalidCredentials)? <= now {
        return Err(AuthError::InvalidCredentials);
    }
    Ok(Principal::from_claims(claims))
}

/// Issues an HS256 JWT from server-owned claims and a nonempty signing key.
///
/// The caller supplies the subject, expiry (UNIX seconds) and grants explicitly.
/// No claims are copied from a request or added implicitly. Use a securely
/// generated secret; key distribution and rotation belong to the application.
/// This does not replace the Better Auth adapter's provider-specific JWT format.
///
/// # Errors
/// Returns [`AuthError::InvalidCredentials`] for an empty key/subject or when
/// serialization/signing fails. The caller owns expiry policy; an already
/// expired token can be issued but will fail verification.
pub fn issue_jwt_hs256(claims: &JwtClaims, signature_key: &[u8]) -> Result<String, AuthError> {
    if signature_key.is_empty() || claims.sub.is_empty() {
        return Err(AuthError::InvalidCredentials);
    }
    jsonwebtoken::encode(
        &jsonwebtoken::Header::new(Algorithm::HS256),
        claims,
        &jsonwebtoken::EncodingKey::from_secret(signature_key),
    )
    .map_err(|_| AuthError::InvalidCredentials)
}

/// Computes the base64-encoded HMAC-SHA256 signature for `payload`.
/// # Panics
///
/// Panics if the HMAC implementation rejects the key; SHA-256 accepts any key size.
#[must_use]
pub fn hmac_sign(secret: &[u8], payload: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts keys of any size");
    mac.update(payload);
    STANDARD.encode(mac.finalize().into_bytes())
}

/// Verifies a base64-encoded HMAC-SHA256 signature in constant time.
/// # Panics
///
/// Panics if the HMAC implementation rejects the key; SHA-256 accepts any key size.
#[must_use]
pub fn hmac_verify(secret: &[u8], payload: &[u8], signature: &str) -> bool {
    let Ok(provided) = STANDARD.decode(signature) else {
        return false;
    };

    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts keys of any size");
    mac.update(payload);
    let expected = mac.finalize().into_bytes();

    expected.len() == provided.len() && bool::from(expected[..].ct_eq(provided.as_slice()))
}

/// Enforces an [`RbacConfig`] against a principal.
///
/// # Errors
///
/// Returns [`AuthError::Forbidden`] describing the first failed rule.
pub fn authorize(principal: &Principal, rbac: &RbacConfig) -> Result<(), AuthError> {
    if let Some(permission) = rbac
        .forbidden_permissions
        .iter()
        .find(|permission| principal.permissions.contains(permission))
    {
        return Err(AuthError::Forbidden(format!(
            "permission `{permission}` is forbidden"
        )));
    }

    if !rbac.allowed_permissions.is_empty()
        && !rbac
            .allowed_permissions
            .iter()
            .any(|permission| principal.permissions.contains(permission))
    {
        return Err(AuthError::Forbidden(
            "missing a required permission".to_owned(),
        ));
    }

    if let Some(role) = rbac
        .forbidden_roles
        .iter()
        .find(|role| principal.roles.contains(role))
    {
        return Err(AuthError::Forbidden(format!("role `{role}` is forbidden")));
    }

    if !rbac.allowed_roles.is_empty()
        && !rbac
            .allowed_roles
            .iter()
            .any(|role| principal.roles.contains(role))
    {
        return Err(AuthError::Forbidden("missing a required role".to_owned()));
    }

    if let Some(required_scope) = &rbac.required_scope
        && !scope_satisfied(principal, required_scope, &rbac.scope_hierarchy)
    {
        return Err(AuthError::Forbidden(format!(
            "missing required scope `{required_scope}`"
        )));
    }

    if let Some(feature) = rbac
        .required_features
        .iter()
        .find(|feature| !principal.features.contains(feature))
    {
        return Err(AuthError::Forbidden(format!(
            "missing required feature `{feature}`"
        )));
    }

    if rbac.require_active_subscription && !principal.active_subscription {
        return Err(AuthError::Forbidden(
            "an active subscription is required".to_owned(),
        ));
    }

    Ok(())
}

/// Returns whether a principal satisfies `required_scope`, honoring a hierarchy
/// ordered from most to least privileged.
#[must_use]
pub fn scope_satisfied(principal: &Principal, required_scope: &str, hierarchy: &[String]) -> bool {
    if principal.scopes.iter().any(|scope| scope == required_scope) {
        return true;
    }

    let Some(required_rank) = hierarchy.iter().position(|scope| scope == required_scope) else {
        return false;
    };

    principal.scopes.iter().any(|scope| {
        hierarchy
            .iter()
            .position(|candidate| candidate == scope)
            .is_some_and(|rank| rank <= required_rank)
    })
}

#[cfg(test)]
mod tests;
