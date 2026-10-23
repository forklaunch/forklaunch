//! Fixed direct Subscription creation transport. No managed/raw forwarding or Connect.
use super::{BillingTransport, StripeBillingClient, context::VerifiedStripeContext};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::http::provider_http::provider_status_error;
use reqwest::Method;
use serde_json::Value;

impl StripeBillingClient {
    pub(crate) async fn subscription_creation_request(
        &self,
        context: &VerifiedStripeContext,
        method: Method,
        segments: &[&str],
        key: Option<&str>,
        fields: &Value,
    ) -> BlueprintResult<Value> {
        self.direct_creation_version()?;
        let invalid = || {
            BlueprintError::new(
                "billing_bound_subscription_invalid",
                "Invalid bound subscription creation",
            )
        };
        let BillingTransport::Direct { http, secret } = &self.transport else {
            return Err(invalid());
        };
        let allowed = match (method.clone(), segments) {
            (Method::GET, ["v1", "customers" | "plans" | "subscriptions" | "events", id]) => {
                super::super::bound_plan_creation::safe_reference(id)
            }
            (Method::POST, ["v1", "subscriptions"]) => true,
            _ => false,
        };
        if !allowed || (method == Method::POST) != key.is_some() {
            return Err(invalid());
        }
        let mut request = http
            .request(method.clone(), segments)?
            .bearer_auth(secret.as_str())
            .header("stripe-version", self.api_version.as_str());
        if let Some(key) = key {
            if !key.starts_with("fl-subscription-")
                || key.len() > 255
                || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            {
                return Err(invalid());
            }
            request = request
                .header("idempotency-key", key)
                .form(&super::form_fields(fields)?);
        } else {
            request = request.query(&super::form_fields(fields)?);
        }
        context.ensure_current(self)?;
        let response = http.execute_json(request).await?;
        context.ensure_current(self)?;
        if !(200..300).contains(&response.status) {
            return Err(provider_status_error("stripe", "billing", response.status));
        }
        Ok(response.body)
    }
}
