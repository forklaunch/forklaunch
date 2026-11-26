//! Declaration half of the session-authorization fixture: compiled by the
//! generated service's dependency-free contract host and by the service.
use forklaunch_core::contract::{AccessLevel, AuthConfig, BodyContract, JwtAuthConfig, RouteContract};
pub fn contracts() -> Vec<RouteContract> {
    [
        "permissions",
        "scope",
        "features",
        "unconfigured",
        "basic",
        "resource",
        "jwt-resource",
        "hmac",
        "session",
        "missing-session-callback",
    ]
    .into_iter()
    .map(|name| {
        RouteContract::get(
            format!("Check {name}"),
            "Session authorization fixture",
            format!("/policy/{name}"),
        )
        .with_auth(if name == "hmac" {
            AuthConfig::hmac()
        } else if name == "basic" || name == "resource" {
            AuthConfig::basic()
        } else {
            AuthConfig::jwt(JwtAuthConfig::SignatureKey)
        })
        .with_access(if name == "hmac" {
            AccessLevel::Internal
        } else if name == "scope" || name == "features" || name == "basic" {
            AccessLevel::Authenticated
        } else {
            AccessLevel::Protected
        })
        .with_response(200, Some(BodyContract::json("PolicyResponse")))
    })
    .collect()
}
