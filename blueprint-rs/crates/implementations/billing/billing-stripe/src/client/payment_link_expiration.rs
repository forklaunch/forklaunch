//! Fixed metadata-only operation, never arbitrary provider updates.
use super::{BillingTransport, StripeBillingClient, context::VerifiedStripeContext};
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::http::provider_http::provider_status_error;
use reqwest::Method;
use serde_json::{Value, json};
impl StripeBillingClient {
    pub(crate) async fn retrieve_payment_link_expiration_event(
        &self,
        context: &VerifiedStripeContext,
        id: &str,
    ) -> BlueprintResult<Value> {
        self.direct_creation_version()?;
        if !id.starts_with("evt_")
            || id.len() <= 4
            || id.len() > 128
            || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(crate::bound_payment_link_expiration::invalid());
        }
        context.ensure_current(self)?;
        let event = self
            .retrieve(super::StripeResource::Event, id, &json!({}))
            .await?;
        context.ensure_current(self)?;
        Ok(event)
    }

    pub(crate) async fn expire_payment_link_metadata(
        &self,
        context: &VerifiedStripeContext,
        provider_id: &str,
        key: &str,
    ) -> BlueprintResult<Value> {
        self.direct_creation_version()?;
        let invalid = crate::bound_payment_link_expiration::invalid;
        if !provider_id.starts_with("plink_")
            || provider_id.len() <= 6
            || provider_id.len() > 128
            || !provider_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || !key.starts_with("fl-plink-expire-")
            || key.len() > 255
            || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(invalid());
        }
        let BillingTransport::Direct { http, secret } = &self.transport else {
            return Err(invalid());
        };
        let fields = json!({"metadata":{"status":"EXPIRED"}});
        let request = http
            .request(Method::POST, &["v1", "payment_links", provider_id])?
            .bearer_auth(secret.as_str())
            .header("stripe-version", self.api_version.as_str())
            .header("idempotency-key", key)
            .form(&super::form_fields(&fields)?);
        context.ensure_current(self)?;
        let response = http.execute_json(request).await?;
        context.ensure_current(self)?;
        if !(200..300).contains(&response.status) {
            return Err(provider_status_error("stripe", "billing", response.status));
        }
        Ok(response.body)
    }
}
