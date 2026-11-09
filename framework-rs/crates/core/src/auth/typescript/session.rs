//! Ordered, application-owned session validation and grant callbacks.
use super::{TypeScriptRbacPolicy, configured, denied, granted};
use crate::http::{ErrorKind, ErrorResponse};
use forklaunch_core::contract::AccessLevel;
use serde_json::Value;
use std::future::Future;

/// Trusted session validation and asynchronous grant lookup for one request.
/// Implementations can retain server-owned request context, stores and tenant IDs.
/// Credentials must already have been verified before calling the session policy.
/// Returned grants must come from trusted application logic, not request headers.
///
/// A missing callback returns `None`. Empty grants return `Some(Vec::new())`;
/// these are deliberately distinct. Callback errors are explicit public responses;
/// implementations must redact private provider failures. No lookup is retried.
pub trait TypeScriptSessionResolver: Sync {
    /// Check the session schema without replacing its claims with parsed,
    /// transformed or defaulted values, matching the TypeScript middleware.
    ///
    /// # Errors
    /// Return the application's public validation error, normally status 400
    /// and `Invalid session: ...`. Error formatting remains validator-owned.
    fn validate_session(&self, _claims: &Value) -> Result<(), ErrorResponse> {
        Ok(())
    }

    /// Resolve scopes; an absent callback skips scope checking in this profile.
    fn scopes(
        &self,
        _claims: &Value,
    ) -> impl Future<Output = Result<Option<Vec<String>>, ErrorResponse>> + Send {
        async { Ok(None) }
    }

    /// Resolve permissions only when configured and applicable to route access.
    fn permissions(
        &self,
        _claims: &Value,
    ) -> impl Future<Output = Result<Option<Vec<String>>, ErrorResponse>> + Send {
        async { Ok(None) }
    }

    /// Resolve roles only when permission rules do not take precedence.
    fn roles(
        &self,
        _claims: &Value,
    ) -> impl Future<Output = Result<Option<Vec<String>>, ErrorResponse>> + Send {
        async { Ok(None) }
    }

    /// Resolve active subscription state when the policy requires it.
    fn subscription(
        &self,
        _claims: &Value,
    ) -> impl Future<Output = Result<Option<bool>, ErrorResponse>> + Send {
        async { Ok(None) }
    }

    /// Resolve features after all preceding checks have passed.
    fn features(
        &self,
        _claims: &Value,
    ) -> impl Future<Output = Result<Option<Vec<String>>, ErrorResponse>> + Send {
        async { Ok(None) }
    }
}

impl TypeScriptRbacPolicy {
    /// Authorize already verified claims using TypeScript callback ordering.
    /// Public access skips all work. Otherwise validation precedes scopes,
    /// permissions or roles, subscription and features. A rejection stops later
    /// callbacks; authenticated/internal access skips permissions and roles.
    /// Original claims remain unchanged, including unknown fields.
    ///
    /// This does not authenticate credentials or create/bind a principal. Call it
    /// inside the request authenticator before returning an authenticated session;
    /// configure route authorization consistently with this complete policy.
    /// Dropping the future stops polling the current callback, without retrying it.
    ///
    /// # Errors
    /// Missing required callbacks return the TypeScript 500 messages. Failed
    /// grants return the corresponding 401/403; validation and callback errors
    /// propagate as explicitly public responses chosen by the application.
    pub async fn authorize_session(
        &self,
        claims: &Value,
        access: Option<AccessLevel>,
        resolver: &impl TypeScriptSessionResolver,
    ) -> Result<(), ErrorResponse> {
        if access == Some(AccessLevel::Public) {
            return Ok(());
        }
        resolver.validate_session(claims)?;
        if let Some(required) = &self.required_scope
            && let Some(scopes) = resolver.scopes(claims).await?
            && let Some(hierarchy) = &self.scope_hierarchy
            && !required.is_empty()
            && (!scopes.contains(required) || scopes.iter().any(|scope| !hierarchy.contains(scope)))
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
                let permissions = resolver
                    .permissions(claims)
                    .await?
                    .ok_or_else(|| missing("No permission surfacing function provided."))?;
                if !granted(
                    &permissions,
                    self.allowed_permissions.as_deref(),
                    self.forbidden_permissions.as_deref(),
                ) {
                    return Err(denied("Invalid Authorization permissions."));
                }
            } else if configured(
                self.allowed_roles.as_deref(),
                self.forbidden_roles.as_deref(),
            ) {
                let roles = resolver
                    .roles(claims)
                    .await?
                    .ok_or_else(|| missing("No role surfacing function provided."))?;
                if !granted(
                    &roles,
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
        if self.require_active_subscription {
            let active = resolver
                .subscription(claims)
                .await?
                .ok_or_else(|| missing("No subscription surfacing function provided."))?;
            if !active {
                return Err(denied("Active subscription required."));
            }
        }
        if !self.required_features.is_empty() {
            let features = resolver
                .features(claims)
                .await?
                .ok_or_else(|| missing("No features surfacing function provided."))?;
            if self
                .required_features
                .iter()
                .any(|feature| !features.contains(feature))
            {
                return Err(denied("Required features not available."));
            }
        }
        Ok(())
    }
}

fn missing(message: &str) -> ErrorResponse {
    ErrorResponse::new(ErrorKind::Internal, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Missing;
    impl TypeScriptSessionResolver for Missing {}

    #[tokio::test]
    async fn absent_callbacks_keep_scope_and_required_grant_behavior_distinct() {
        let claims = json!({"sub":"already-verified"});
        TypeScriptRbacPolicy {
            required_scope: Some("team".into()),
            scope_hierarchy: Some(vec![]),
            ..TypeScriptRbacPolicy::default()
        }
        .authorize_session(&claims, None, &Missing)
        .await
        .unwrap();
        for (policy, message) in [
            (
                TypeScriptRbacPolicy {
                    allowed_permissions: Some(vec!["read".into()]),
                    ..TypeScriptRbacPolicy::default()
                },
                "No permission surfacing function provided.",
            ),
            (
                TypeScriptRbacPolicy {
                    allowed_roles: Some(vec!["admin".into()]),
                    ..TypeScriptRbacPolicy::default()
                },
                "No role surfacing function provided.",
            ),
            (
                TypeScriptRbacPolicy {
                    require_active_subscription: true,
                    ..TypeScriptRbacPolicy::default()
                },
                "No subscription surfacing function provided.",
            ),
            (
                TypeScriptRbacPolicy {
                    required_features: vec!["one".into()],
                    ..TypeScriptRbacPolicy::default()
                },
                "No features surfacing function provided.",
            ),
        ] {
            let error = policy
                .authorize_session(&claims, None, &Missing)
                .await
                .unwrap_err();
            assert_eq!(error.to_response().status_code, 500);
            assert_eq!(error.message, message);
            policy
                .authorize_session(&claims, Some(AccessLevel::Public), &Missing)
                .await
                .unwrap();
        }
    }

    struct Pending {
        scopes: AtomicUsize,
        features: AtomicUsize,
    }
    impl TypeScriptSessionResolver for Pending {
        async fn scopes(&self, _: &Value) -> Result<Option<Vec<String>>, ErrorResponse> {
            self.scopes.fetch_add(1, Ordering::SeqCst);
            std::future::pending().await
        }
        async fn features(&self, _: &Value) -> Result<Option<Vec<String>>, ErrorResponse> {
            self.features.fetch_add(1, Ordering::SeqCst);
            Ok(Some(vec!["one".into()]))
        }
    }
    #[tokio::test]
    async fn cancellation_never_starts_later_callbacks_or_retries_current_one() {
        let resolver = Pending {
            scopes: AtomicUsize::new(0),
            features: AtomicUsize::new(0),
        };
        let policy = TypeScriptRbacPolicy {
            required_scope: Some("team".into()),
            required_features: vec!["one".into()],
            ..TypeScriptRbacPolicy::default()
        };
        let claims = json!({"sub":"verified"});
        let mut future = Box::pin(policy.authorize_session(&claims, None, &resolver));
        std::future::poll_fn(|context| {
            assert!(future.as_mut().poll(context).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        drop(future);
        assert_eq!(resolver.scopes.load(Ordering::SeqCst), 1);
        assert_eq!(resolver.features.load(Ordering::SeqCst), 0);
    }
}
