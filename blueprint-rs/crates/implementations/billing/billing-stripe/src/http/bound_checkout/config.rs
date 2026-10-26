//! Explicit server-owned profile; no DTO/cache field can supply this authority.
use crate::bound_checkout_creation::BoundCheckoutCreationProfile;
use forklaunch_billing_base::services::postgres::StripeBindingScope;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use serde_json::Value;
use uuid::Uuid;

/// Opt-in surface selector, independent of request date coercion.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum BillingIdentityProfile {
    #[default]
    Legacy,
    BoundCheckoutV1,
    BoundBillingV1,
}
impl std::str::FromStr for BillingIdentityProfile {
    type Err = BlueprintError;
    fn from_str(value: &str) -> BlueprintResult<Self> {
        match value {
            "legacy" => Ok(Self::Legacy),
            "bound-checkout-v1" => Ok(Self::BoundCheckoutV1),
            "bound-billing-v1" => Ok(Self::BoundBillingV1),
            _ => Err(configuration()),
        }
    }
}
/// No Debug/Serde: configuration is never reflected in HTTP errors or metadata.
pub struct BoundCheckoutConfig {
    pub(super) scope: StripeBindingScope,
    pub(super) profile: BoundCheckoutCreationProfile,
}
pub(super) fn configuration() -> BlueprintError {
    BlueprintError::new(
        "billing_configuration",
        "Invalid bound Checkout configuration",
    )
}
fn exact(value: &Value, keys: &[&str]) -> BlueprintResult<()> {
    let object = value.as_object().ok_or_else(configuration)?;
    if object.len() != keys.len() || keys.iter().any(|key| !object.contains_key(*key)) {
        return Err(configuration());
    }
    Ok(())
}
fn string(value: &Value) -> BlueprintResult<String> {
    value.as_str().map(str::to_owned).ok_or_else(configuration)
}
fn id(value: &Value) -> BlueprintResult<Uuid> {
    Uuid::parse_str(value.as_str().ok_or_else(configuration)?).map_err(|_| configuration())
}
impl BoundCheckoutConfig {
    /// Parses bounded server-owned configuration only. Exact keys prevent silently
    /// ignored fee/delegation/policy options. URLs and Plan provenance are fixed.
    /// # Errors
    /// Sanitized failure for invalid/unknown fields, excessive input or bad scope.
    pub fn from_json(text: &str) -> BlueprintResult<Self> {
        if text.len() > 65536 {
            return Err(configuration());
        }
        let value: Value = serde_json::from_str(text).map_err(|_| configuration())?;
        exact(
            &value,
            &[
                "deploymentLabel",
                "accountId",
                "livemode",
                "plans",
                "successUrl",
                "cancelUrl",
            ],
        )?;
        let scope = StripeBindingScope::administrative(
            string(&value["deploymentLabel"])?,
            string(&value["accountId"])?,
            value["livemode"].as_bool().ok_or_else(configuration)?,
        )
        .map_err(|_| configuration())?;
        let rows = value["plans"].as_array().ok_or_else(configuration)?;
        if rows.is_empty() || rows.len() > 1000 {
            return Err(configuration());
        }
        let mut plans = Vec::with_capacity(rows.len());
        for row in rows {
            exact(row, &["localPlanId", "creationOperationId"])?;
            plans.push((id(&row["localPlanId"])?, id(&row["creationOperationId"])?));
        }
        let profile = BoundCheckoutCreationProfile::trusted(
            scope.clone(),
            plans,
            string(&value["successUrl"])?,
            string(&value["cancelUrl"])?,
        )
        .map_err(|_| configuration())?;
        Ok(Self { scope, profile })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn valid() -> Value {
        json!({"deploymentLabel":"owned","accountId":"acct_owned","livemode":false,"plans":[{"localPlanId":Uuid::new_v4().to_string(),"creationOperationId":Uuid::new_v4().to_string()}],"successUrl":"https://example.test/success","cancelUrl":"https://example.test/cancel"})
    }
    #[test]
    fn server_configuration_is_finite_and_sanitized() {
        assert!(BoundCheckoutConfig::from_json(&valid().to_string()).is_ok());
        for key in [
            "deploymentLabel",
            "accountId",
            "livemode",
            "plans",
            "successUrl",
            "cancelUrl",
        ] {
            let mut value = valid();
            value.as_object_mut().unwrap().remove(key);
            let error = BoundCheckoutConfig::from_json(&value.to_string())
                .err()
                .unwrap();
            assert_eq!(error.message, "Invalid bound Checkout configuration");
        }
        for (key, value) in [
            ("stripeFields", json!({"secret":"must-not-leak"})),
            ("plans", json!([])),
            ("livemode", json!("false")),
            ("successUrl", json!("http://bad")),
            ("accountId", json!("secret-must-not-leak")),
        ] {
            let mut config = valid();
            config[key] = value;
            let error = BoundCheckoutConfig::from_json(&config.to_string())
                .err()
                .unwrap();
            assert_eq!(error.message, "Invalid bound Checkout configuration");
        }
        assert!(BoundCheckoutConfig::from_json(&" ".repeat(65537)).is_err());
        assert!("unknown".parse::<BillingIdentityProfile>().is_err());
        assert!(matches!(
            "legacy".parse::<BillingIdentityProfile>().unwrap(),
            BillingIdentityProfile::Legacy
        ));
    }
}
