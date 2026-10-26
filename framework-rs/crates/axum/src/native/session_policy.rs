//! Native admission using the existing verified-session and authorization hooks.
use super::{EndpointError, EndpointPolicy, PolicyCapabilities};
use crate::session::{AuthenticatedSession, Authenticator, Authorizer};
use axum::{
    extract::{FromRequestParts, Path},
    http::request::Parts,
};
use forklaunch_core::{
    auth::{AuthError, typescript::TypeScriptRbacPolicy},
    contract::{AccessLevel, AuthConfig, RouteContract, bindings::AuthorizationPolicy},
    http::{FrameworkRequest, RequestContext},
};
use std::{collections::HashMap, sync::Arc};

/// Reuses trusted credential verification and verified-session authorization.
/// The verifier must authenticate credentials; unverified JWT decoding is never
/// sufficient. This adapter deliberately cannot admit signed internal routes.
#[derive(Clone)]
pub struct SessionPolicy {
    authenticate: Authenticator,
    authorize: Authorizer,
}
impl SessionPolicy {
    #[must_use]
    pub fn new(authenticate: Authenticator, authorize: Authorizer) -> Self {
        Self {
            authenticate,
            authorize,
        }
    }
    /// Reuse the framework's TypeScript permissions/roles/scope/entitlement rules.
    #[must_use]
    pub fn with_typescript_rules(authenticate: Authenticator, rules: TypeScriptRbacPolicy) -> Self {
        Self::new(
            authenticate,
            Arc::new(move |session, route| rules.authorize(&session.principal, Some(route.access))),
        )
    }
}
impl AuthorizationPolicy for SessionPolicy {}
impl<S: Send + Sync + 'static> EndpointPolicy<S> for SessionPolicy {
    const CAPABILITIES: PolicyCapabilities = PolicyCapabilities::Authorized;
    type Context = AuthenticatedSession;
    async fn authorize(
        &self,
        parts: &mut Parts,
        state: &S,
        route: &RouteContract,
    ) -> Result<Self::Context, EndpointError> {
        if route.access == AccessLevel::Internal || matches!(route.auth, Some(AuthConfig::Hmac(_)))
        {
            return Err(EndpointError::Forbidden);
        }
        // Never let a stale extension serve as a second authentication source.
        parts.extensions.remove::<AuthenticatedSession>();
        let mut request = request(parts, route)?;
        if let Ok(Path(params)) =
            Path::<HashMap<String, String>>::from_request_parts(parts, state).await
        {
            request.params = params.into_iter().collect();
        }
        let session =
            (self.authenticate)(request, route.clone())
                .await
                .map_err(|error| match error {
                    AuthError::Forbidden(_) => EndpointError::Forbidden,
                    _ => EndpointError::Unauthorized,
                })?;
        (self.authorize)(&session, route).map_err(|error| {
            match error.to_response().status_code {
                401 => EndpointError::Unauthorized,
                403 => EndpointError::Forbidden,
                503 => EndpointError::Unavailable,
                _ => EndpointError::InvalidResponse,
            }
        })?;
        parts.extensions.insert(session.clone());
        Ok(session)
    }
}
fn request(parts: &Parts, _route: &RouteContract) -> Result<FrameworkRequest, EndpointError> {
    let mut request = FrameworkRequest::new(
        forklaunch_core::contract::HttpMethod::ALL
            .into_iter()
            .find(|method| method.as_str() == parts.method.as_str())
            .ok_or(EndpointError::InvalidRequest)?,
        parts.uri.path(),
        RequestContext::new(uuid::Uuid::new_v4().to_string()),
    );
    request.raw_path = Some(parts.uri.path().to_owned());
    request.raw_query = parts.uri.query().map(str::to_owned);
    if let Some(query) = &request.raw_query {
        request.query = serde_urlencoded::from_str::<Vec<(String, String)>>(query)
            .map_err(|_| EndpointError::InvalidRequest)?
            .into_iter()
            .collect();
    }
    for name in parts.headers.keys() {
        let values = parts.headers.get_all(name);
        if matches!(name.as_str(), "authorization" | "cookie") && values.iter().count() != 1 {
            return Err(EndpointError::Unauthorized);
        }
        let values = values
            .iter()
            .map(|value| value.to_str().map(str::to_owned))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| EndpointError::InvalidRequest)?;
        request
            .headers
            .insert(name.as_str().to_owned(), values.join(", "));
    }
    Ok(request)
}
#[cfg(test)]
mod tests;
