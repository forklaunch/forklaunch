//! Business synchronization for verified Stripe and platform payment events.
//! Authentication and durable delivery receipts belong to the inbound adapter;
//! call this service only after verifying the exact raw request body.
use crate::{client::StripeResource, services::StripeBillingService};
use chrono::{DateTime, Utc};
use forklaunch_billing_base::services::BillingStore;
use forklaunch_billing_interface::services::{
    BillingPortalService, CheckoutSessionService, PaymentLinkService, PlanService,
    SubscriptionService,
};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};

#[derive(Debug, Clone, Deserialize)]
pub struct BillingWebhookEvent {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: BillingWebhookData,
}
#[derive(Debug, Clone, Deserialize)]
pub struct BillingWebhookData {
    pub object: Value,
}
impl BillingWebhookEvent {
    /// # Errors
    /// Rejects malformed event identity or a non-object provider payload.
    pub fn validate(&self) -> BlueprintResult<()> {
        for value in [&self.id, &self.event_type] {
            if value.is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
                return Err(invalid());
            }
        }
        if !self.data.object.is_object() {
            return Err(invalid());
        }
        Ok(())
    }
}
impl<S: BillingStore> StripeBillingService<S> {
    /// Applies a verified event. Delivery deduplication must surround this call.
    /// # Errors
    /// Returns sanitized provider, persistence and malformed-event failures.
    #[allow(clippy::too_many_lines)]
    pub async fn synchronize_event(
        &self,
        event: &BillingWebhookEvent,
        default_party_type: &str,
    ) -> BlueprintResult<()> {
        event.validate()?;
        let object = &event.data.object;
        let base = self.base();
        match event.event_type.as_str() {
            "billing_portal.session.created" => {
                let expires = portal_expiry(&object["created"])?;
                base.create_billing_portal_session(decode(json!({"id":identifier(object)?,
                    "customerId":identifier(&object["customer"])?,"expiresAt":expires,
                    "uri":string(object,"url")?}))?)
                    .await?;
            }
            "checkout.session.expired" => self.handle_checkout_failure(identifier(object)?).await?,
            "checkout.session.completed" => {
                self.handle_checkout_success(identifier(object)?).await?;
            }
            "payment_link.created" | "payment_link.updated" => {
                let amount = match object["line_items"]["data"].as_array() {
                    Some(items) => items.iter().try_fold(0.0, |sum, item| {
                        let value = sum + item["amount_total"].as_f64().ok_or_else(invalid)?;
                        if value.is_finite() {
                            Ok(value)
                        } else {
                            Err(invalid())
                        }
                    })?,
                    None => 0.0,
                };
                let mut data = json!({"id":identifier(object)?,"amount":amount,
                    "status":if event.event_type.ends_with("created") { "CREATED" } else { "UPDATED" }});
                copy_present(object, &mut data, "payment_method_types", "paymentMethods");
                copy_present(object, &mut data, "currency", "currency");
                if event.event_type.ends_with("created") {
                    base.create_payment_link(decode(data)?).await?;
                } else {
                    base.update_payment_link(decode(data)?).await?;
                }
            }
            "plan.created" | "plan.updated" | "price.created" | "price.updated" => {
                let price = event.event_type.starts_with("price.");
                if object["product"].is_null()
                    || object[if price { "unit_amount" } else { "amount" }].is_null()
                    || (price && !object["recurring"].is_object())
                {
                    if price {
                        return Ok(());
                    }
                    return Err(invalid());
                }
                let product = self
                    .client()
                    .retrieve(
                        StripeResource::Product,
                        identifier(&object["product"])?,
                        &json!({}),
                    )
                    .await?;
                let data = json!({"id":identifier(object)?,"billingProvider":"stripe",
                    "cadence":if price { &object["recurring"]["interval"] } else { &object["interval"] },
                    "currency":object["currency"],"active":if price {
                        boolean(object,"active")? && boolean(&product,"active")?
                    } else { boolean(&product,"active")? },
                    "name":string(&product,"name")?,
                    "price":object[if price {"unit_amount"} else {"amount"}],
                    "externalId":identifier(object)?,"features":extract_features(&product)});
                if event.event_type.ends_with("created") {
                    base.create_plan(decode(data)?).await?;
                } else {
                    base.update_plan(decode(data)?).await?;
                }
            }
            "plan.deleted" => self.delete_plan(identifier(object)?).await?,
            "product.created" | "product.updated" => {
                let fields = json!({"product":identifier(object)?});
                let features = extract_features(object);
                for resource in [StripeResource::Plan, StripeResource::Price] {
                    for item in self.client().list_pages(resource, &fields).await? {
                        let data = json!({"id":identifier(&item)?,"features":features,
                            "name":string(object,"name")?,"active":boolean(object,"active")?
                            && (resource != StripeResource::Price || boolean(&item,"active")?)});
                        // TS intentionally continues after an individual local update fails.
                        let _ = base.update_plan(decode(data)?).await;
                    }
                }
            }
            "customer.subscription.created" | "customer.subscription.updated" => {
                let product_id = string(&object["items"]["data"][0]["plan"], "id")?;
                let external_id = identifier(object)?;
                let created = event.event_type.ends_with("created");
                let existing = if created {
                    None
                } else {
                    base.store()
                        .find_subscription_id_by_external_id(external_id)
                        .await?
                };
                let status = string(object, "status")?;
                let start = object["created"]
                    .as_i64()
                    .and_then(|v| DateTime::<Utc>::from_timestamp(v, 0))
                    .ok_or_else(invalid)?;
                let mut data = json!({"active":created || matches!(status,"active"|"trialing"),
                    "productId":product_id,"externalId":external_id,"billingProvider":"stripe",
                    "startDate":start,"status":status});
                if let Some(description) = object.get("description").filter(|v| !v.is_null()) {
                    data["description"] = description.clone();
                }
                if let Some(cancel_at) = object["cancel_at"].as_i64().filter(|v| *v != 0) {
                    data["endDate"] =
                        json!(DateTime::<Utc>::from_timestamp(cancel_at, 0).ok_or_else(invalid)?);
                }
                if let Some(id) = existing {
                    data["id"] = json!(id);
                    base.update_subscription(decode(data)?).await?;
                } else {
                    if created {
                        data["id"] = json!(external_id);
                    }
                    data["partyId"] = json!(identifier(&object["customer"])?);
                    data["partyType"] = json!(default_party_type);
                    base.create_subscription(decode(data)?).await?;
                }
            }
            "customer.subscription.deleted"
            | "customer.subscription.paused"
            | "customer.subscription.resumed" => {
                if let Some(id) = base
                    .store()
                    .find_subscription_id_by_external_id(identifier(object)?)
                    .await?
                {
                    match event.event_type.as_str() {
                        "customer.subscription.deleted" => base.delete_subscription(&id).await?,
                        "customer.subscription.paused" => base.cancel_subscription(&id).await?,
                        _ => base.resume_subscription(&id).await?,
                    }
                }
            }
            _ => (),
        }
        Ok(())
    }
}
// Stripe uses Unix seconds. Match TS's four-digit RFC3339 application expiry
// range; the provider URL's validity and configured cache TTL remain separate.
#[allow(clippy::cast_possible_truncation)] // Integral/range checks precede the exact conversion.
fn portal_expiry(created: &Value) -> BlueprintResult<DateTime<Utc>> {
    // JSON numbers with whole-valued decimal/exponent spellings have the same
    // semantics as integers in TS. This bound is below f64's exact integer limit.
    let seconds = created.as_f64().ok_or_else(invalid)?;
    if !seconds.is_finite()
        || seconds.fract() != 0.0
        || !(0.0..=253_402_300_499.0).contains(&seconds)
    {
        return Err(invalid());
    }
    (seconds as i64)
        .checked_add(300)
        .and_then(|value| DateTime::<Utc>::from_timestamp(value, 0))
        .ok_or_else(invalid)
}
fn extract_features(product: &Value) -> Vec<String> {
    let Some(text) = product["metadata"]["features"].as_str() else {
        return Vec::new();
    };
    if let Ok(Value::Array(values)) = serde_json::from_str::<Value>(text) {
        return values
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
    }
    text.split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect()
}
fn decode<T: DeserializeOwned>(data: Value) -> BlueprintResult<T> {
    serde_json::from_value(data).map_err(|_| invalid())
}
fn identifier(value: &Value) -> BlueprintResult<&str> {
    let id = value
        .as_str()
        .or_else(|| value["id"].as_str())
        .ok_or_else(invalid)?;
    if id.is_empty() || id.len() > 1024 || id.chars().any(char::is_control) {
        return Err(invalid());
    }
    Ok(id)
}
fn string<'a>(value: &'a Value, key: &str) -> BlueprintResult<&'a str> {
    value[key].as_str().ok_or_else(invalid)
}
fn boolean(value: &Value, key: &str) -> BlueprintResult<bool> {
    value[key].as_bool().ok_or_else(invalid)
}
fn copy_present(source: &Value, target: &mut Value, from: &str, to: &str) {
    if let Some(value) = source.get(from) {
        target[to] = value.clone();
    }
}
fn invalid() -> BlueprintError {
    BlueprintError::new(
        "billing_invalid_webhook",
        "Billing webhook payload is invalid",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_expiry_uses_checked_unix_seconds() {
        for (created, expires) in [
            (0_i64, "1970-01-01T00:05:00+00:00"),
            (1_700_000_000, "2023-11-14T22:18:20+00:00"),
            (253_402_300_499, "9999-12-31T23:59:59+00:00"),
        ] {
            assert_eq!(
                portal_expiry(&json!(created)).unwrap().to_rfc3339(),
                expires
            );
        }
        for (lexeme, expires) in [
            ("1700000000.0", "2023-11-14T22:18:20+00:00"),
            ("1e3", "1970-01-01T00:21:40+00:00"),
            ("-0.0", "1970-01-01T00:05:00+00:00"),
        ] {
            let value: Value = serde_json::from_str(lexeme).unwrap();
            assert_eq!(portal_expiry(&value).unwrap().to_rfc3339(), expires);
        }
        for value in [
            Value::Null,
            json!(false),
            json!(true),
            json!("1700000000"),
            json!(-1),
            json!(1.5),
            json!(253_402_300_500_i64),
            json!(i64::MAX),
            json!(u64::MAX),
            json!(1e30),
            json!({}),
            json!([]),
        ] {
            assert!(portal_expiry(&value).is_err(), "accepted {value}");
        }
    }
}
