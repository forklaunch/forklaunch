//! Fixed direct Customer verification, never a mutation or raw managed proxy.
use super::{BillingTransport, StripeBillingClient, VerifiedStripeContext};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::http::provider_http::provider_status_error;
use reqwest::Method;
use serde_json::Value;
fn invalid() -> BlueprintError {
    BlueprintError::new(
        "billing_customer_registration_invalid",
        "Invalid Stripe customer registration",
    )
}
impl StripeBillingClient {
    pub(crate) fn direct_customer_registration_version(&self) -> BlueprintResult<&str> {
        if !matches!(self.transport, BillingTransport::Direct { .. }) {
            return Err(BlueprintError::new(
                "billing_customer_registration_unsupported",
                "Customer registration requires a direct Stripe client",
            ));
        }
        Ok(&self.api_version)
    }
    pub(crate) async fn verify_registration_customer(
        &self,
        context: &VerifiedStripeContext,
        id: &str,
    ) -> BlueprintResult<()> {
        self.direct_customer_registration_version()?;
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        {
            return Err(invalid());
        }
        let BillingTransport::Direct { http, secret } = &self.transport else {
            return Err(invalid());
        };
        let request = http
            .request(Method::GET, &["v1", "customers", id])?
            .bearer_auth(secret.as_str())
            .header("stripe-version", self.api_version.as_str());
        context.ensure_current(self)?;
        let response = http.execute_json(request).await?;
        context.ensure_current(self)?;
        if !(200..300).contains(&response.status) {
            return Err(provider_status_error("stripe", "billing", response.status));
        }
        validate_customer(&response.body, id, context.livemode())
    }
}
fn validate_customer(value: &Value, id: &str, livemode: bool) -> BlueprintResult<()> {
    if value["id"] != id
        || value["object"] != "customer"
        || value["livemode"].as_bool() != Some(livemode)
        || value
            .get("deleted")
            .is_some_and(|value| value.as_bool() != Some(false))
    {
        return Err(invalid());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn deleted_or_malformed_customer_is_not_registration_evidence() {
        let good = json!({"id":"cus_owned","object":"customer","livemode":false});
        assert!(validate_customer(&good, "cus_owned", false).is_ok());
        for (field, bad) in [
            ("id", json!("other")),
            ("id", json!(null)),
            ("object", json!("account")),
            ("livemode", json!(true)),
            ("livemode", json!(null)),
            ("deleted", json!(true)),
            ("deleted", json!(null)),
            ("deleted", json!("false")),
        ] {
            let mut value = good.clone();
            value[field] = bad;
            assert!(validate_customer(&value, "cus_owned", false).is_err());
        }
    }
}
