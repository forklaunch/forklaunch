//! Provider webhook bookkeeping, independent of provider authentication and business processing.
use forklaunch_core::domain::BlueprintResult;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::future::Future;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookEvent {
    pub provider: String,
    pub provider_event_id: String,
    pub event_type: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WebhookEventOutcome {
    New,
    AlreadyProcessed,
    Retry,
}
/// `begin_processing` durably records receipt; only successful business processing
/// may call `mark_processed`. Controllers separately serialize concurrent delivery.
pub trait WebhookEventStore: Send + Sync {
    fn begin_processing(
        &self,
        event: WebhookEvent,
    ) -> impl Future<Output = BlueprintResult<WebhookEventOutcome>> + Send;
    fn mark_processed(
        &self,
        provider: &str,
        event_id: &str,
    ) -> impl Future<Output = BlueprintResult<()>> + Send;
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PaypalWebhookEvent {
    pub id: String,
    pub event_type: String,
    pub resource: PaypalWebhookResource,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PaypalWebhookResource {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::optional"
    )]
    #[schemars(with = "String")]
    pub id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::optional"
    )]
    #[schemars(with = "PaypalSupplementaryData")]
    pub supplementary_data: Option<PaypalSupplementaryData>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::optional"
    )]
    #[schemars(with = "Vec<PaypalPurchaseUnit>")]
    pub purchase_units: Option<Vec<PaypalPurchaseUnit>>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PaypalSupplementaryData {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::optional"
    )]
    #[schemars(with = "PaypalRelatedIds")]
    pub related_ids: Option<PaypalRelatedIds>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PaypalRelatedIds {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::optional"
    )]
    #[schemars(with = "String")]
    pub order_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PaypalPurchaseUnit {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::optional"
    )]
    #[schemars(with = "PaypalWebhookAmount")]
    pub amount: Option<PaypalWebhookAmount>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PaypalWebhookAmount {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::optional"
    )]
    #[schemars(with = "String")]
    pub value: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::optional"
    )]
    #[schemars(with = "String")]
    pub currency_code: Option<String>,
}
impl PaypalWebhookEvent {
    #[must_use]
    pub fn related_order_id(&self) -> Option<&str> {
        self.resource
            .supplementary_data
            .as_ref()?
            .related_ids
            .as_ref()?
            .order_id
            .as_deref()
    }
    #[must_use]
    pub fn amount(&self) -> Option<&PaypalWebhookAmount> {
        self.resource
            .purchase_units
            .as_ref()?
            .first()?
            .amount
            .as_ref()
    }
    /// Exact nonnegative decimal cents; malformed, overflowing and fractional-cent
    /// provider values cannot authorize an order transition.
    #[must_use]
    pub fn amount_cents(&self) -> Option<i64> {
        let value = self.amount()?.value.as_deref()?;
        let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
        if whole.is_empty()
            || !whole.bytes().all(|byte| byte.is_ascii_digit())
            || fraction.len() > 2
            || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        let cents = match fraction.len() {
            0 => 0,
            1 => fraction.parse::<i64>().ok()?.checked_mul(10)?,
            _ => fraction.parse().ok()?,
        };
        whole
            .parse::<i64>()
            .ok()?
            .checked_mul(100)?
            .checked_add(cents)
    }
}

#[cfg(test)]
mod tests {
    use super::PaypalWebhookEvent;
    use serde_json::json;

    #[test]
    fn provider_amounts_reconcile_only_exact_bounded_cents() {
        for (value, expected) in [
            ("0", Some(0)),
            ("0.01", Some(1)),
            ("7.5", Some(750)),
            ("813.00", Some(81300)),
            ("92233720368547758.07", Some(i64::MAX)),
            ("92233720368547758.08", None),
            ("1.001", None),
            ("1junk", None),
            ("-1", None),
            ("NaN", None),
            (" 1.00", None),
        ] {
            let event: PaypalWebhookEvent = serde_json::from_value(json!({
                "id":"event", "event_type":"CHECKOUT.ORDER.APPROVED",
                "resource":{"purchase_units":[{"amount":{"value":value,"currency_code":"USD"}}]}
            }))
            .unwrap();
            assert_eq!(event.amount_cents(), expected, "{value}");
        }
        assert!(
            serde_json::from_value::<PaypalWebhookEvent>(json!({
                "id":"event", "event_type":"UNKNOWN", "resource":{"purchase_units":null}
            }))
            .is_err()
        );
    }
}
