//! Opt-in TypeScript credential parsing and verified-session authorization.
pub mod basic;
pub mod hmac;
pub mod jwt;
pub mod session;
use crate::auth::Principal;
use crate::http::{ErrorKind, ErrorResponse};
use forklaunch_core::contract::AccessLevel;
use serde::{Deserialize, Serialize};

/// Preserves omitted versus explicitly empty rule sets from TypeScript configs.
/// The principal must already contain server-resolved grants. This does not
/// authenticate credentials, invoke surfacing callbacks, or validate sessions.
/// HMAC routes have no session and should not call this session authorizer.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeScriptRbacPolicy {
    pub allowed_permissions: Option<Vec<String>>,
    pub forbidden_permissions: Option<Vec<String>>,
    pub allowed_roles: Option<Vec<String>>,
    pub forbidden_roles: Option<Vec<String>>,
    pub required_scope: Option<String>,
    #[serde(rename = "scopeHeirarchy", alias = "scopeHierarchy")]
    pub scope_hierarchy: Option<Vec<String>>,
    #[serde(default)]
    pub required_features: Vec<String>,
    #[serde(default)]
    pub require_active_subscription: bool,
}

impl TypeScriptRbacPolicy {
    /// Apply the repository TypeScript middleware's session rules. Permissions
    /// take precedence over roles; authenticated/internal access skips those
    /// two checks, while scope, subscription and feature checks still apply.
    /// A declared hierarchy requires the exact requested scope and rejects
    /// surfaced scopes outside that hierarchy; it does not imply other scopes.
    /// `None` access represents legacy contracts without an access declaration.
    ///
    /// # Errors
    /// Returns the matching 401/403 status and public middleware message.
    /// The caller chooses its HTTP error representation.
    pub fn authorize(
        &self,
        principal: &Principal,
        access: Option<AccessLevel>,
    ) -> Result<(), ErrorResponse> {
        if access == Some(AccessLevel::Public) {
            return Ok(());
        }
        if let (Some(required), Some(hierarchy)) = (&self.required_scope, &self.scope_hierarchy)
            && !required.is_empty()
            && (!principal.scopes.contains(required)
                || principal
                    .scopes
                    .iter()
                    .any(|scope| !hierarchy.contains(scope)))
        {
            return Err(denied("Invalid scope for operation."));
        }
        if !matches!(
            access,
            Some(AccessLevel::Authenticated | AccessLevel::Internal)
        ) {
            if configured(
                self.allowed_permissions.as_deref(),
                self.forbidden_permissions.as_deref(),
            ) {
                if !granted(
                    &principal.permissions,
                    self.allowed_permissions.as_deref(),
                    self.forbidden_permissions.as_deref(),
                ) {
                    return Err(denied("Invalid Authorization permissions."));
                }
            } else if configured(
                self.allowed_roles.as_deref(),
                self.forbidden_roles.as_deref(),
            ) {
                if !granted(
                    &principal.roles,
                    self.allowed_roles.as_deref(),
                    self.forbidden_roles.as_deref(),
                ) {
                    return Err(denied("Invalid Authorization roles."));
                }
            } else if access == Some(AccessLevel::Protected) {
                return Err(ErrorResponse::new(
                    ErrorKind::Unauthorized,
                    "Invalid Authorization method.",
                ));
            }
        }
        if self.require_active_subscription && !principal.active_subscription {
            return Err(denied("Active subscription required."));
        }
        if self
            .required_features
            .iter()
            .any(|feature| !principal.features.contains(feature))
        {
            return Err(denied("Required features not available."));
        }
        Ok(())
    }
}

fn configured(allowed: Option<&[String]>, forbidden: Option<&[String]>) -> bool {
    allowed.is_some_and(|values| !values.is_empty())
        || forbidden.is_some_and(|values| !values.is_empty())
}
fn granted(grants: &[String], allowed: Option<&[String]>, forbidden: Option<&[String]>) -> bool {
    allowed.is_none_or(|values| values.iter().any(|value| grants.contains(value)))
        && forbidden.is_none_or(|values| !values.iter().any(|value| grants.contains(value)))
}
fn denied(message: &str) -> ErrorResponse {
    ErrorResponse::new(ErrorKind::Forbidden, message)
}

fn authorization_token<'a>(
    header: Option<&'a str>,
    token_prefix: &str,
) -> Result<&'a str, ErrorResponse> {
    let header = header.ok_or_else(|| {
        ErrorResponse::new(ErrorKind::Unauthorized, "Authorization token required.")
    })?;
    let mut parts = header.split(' ');
    let prefix = parts.next().unwrap_or_default();
    let token = parts.next().ok_or_else(invalid_token_format)?;
    if prefix.is_empty() || prefix != token_prefix {
        return Err(invalid_token_format());
    }
    Ok(token)
}

fn invalid_token_format() -> ErrorResponse {
    ErrorResponse::new(
        ErrorKind::Unauthorized,
        "Invalid Authorization token format.",
    )
}
