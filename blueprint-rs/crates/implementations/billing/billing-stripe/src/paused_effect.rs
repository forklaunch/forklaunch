//! Acknowledgement of an already-satisfied effect, not causal webhook repair.
use crate::services::StripeBillingService;
use chrono::{DateTime, Utc};
use forklaunch_billing_base::services::{
    BillingKind,
    postgres::{
        PausedEffectAcknowledgement, PendingWebhook, PostgresBillingStore, StripeBindingScope,
    },
};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use serde_json::Value;
use uuid::Uuid;

fn review() -> BlueprintError {
    BlueprintError::new(
        "billing_paused_effect_review_required",
        "Paused subscription effect requires manual review",
    )
}
fn strict_id(value: &Value, prefix: &str) -> BlueprintResult<String> {
    value
        .as_str()
        .filter(|id| {
            id.starts_with(prefix)
                && id.len() > prefix.len()
                && id.len() <= 128
                && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
        .map(str::to_owned)
        .ok_or_else(review)
}
fn customer(value: &Value) -> BlueprintResult<String> {
    if value.is_string() {
        return strict_id(value, "cus_");
    }
    if value["object"] != "customer"
        || value
            .get("deleted")
            .is_some_and(|deleted| deleted.as_bool() != Some(false))
    {
        return Err(review());
    }
    strict_id(&value["id"], "cus_")
}
fn subscription(value: &Value, id: &str, customer_id: &str, livemode: bool) -> BlueprintResult<()> {
    if value["object"] != "subscription"
        || strict_id(&value["id"], "sub_")? != id
        || value["status"] != "paused"
        || value["livemode"].as_bool() != Some(livemode)
        || value["customer"]
            .get("livemode")
            .is_some_and(|mode| mode.as_bool() != Some(livemode))
        || customer(&value["customer"])? != customer_id
    {
        return Err(review());
    }
    Ok(())
}
fn event_evidence(
    pending: &PendingWebhook,
    event: &Value,
    id: &str,
    customer_id: &str,
    livemode: bool,
    connected_account: Option<&str>,
) -> BlueprintResult<()> {
    let started = DateTime::parse_from_str(&pending.started_at, "%Y-%m-%d %H:%M:%S%.f%#z")
        .or_else(|_| DateTime::parse_from_rfc3339(&pending.started_at))
        .map_err(|_| review())?;
    let created = event["created"]
        .as_i64()
        .filter(|created| {
            *created >= 0 && *created <= started.timestamp() && *created <= Utc::now().timestamp()
        })
        .ok_or_else(review)?;
    if Utc::now().timestamp().saturating_sub(created) > 30 * 24 * 60 * 60
        || event["object"] != "event"
        || event["id"] != pending.event_id
        || event["type"] != pending.event_type
        || event["livemode"].as_bool() != Some(livemode)
        || event.get("context").is_some_and(|value| !value.is_null())
        || event
            .get("account")
            .filter(|value| !value.is_null())
            .is_some_and(|value| connected_account.is_none() || value.as_str() != connected_account)
    {
        return Err(review());
    }
    subscription(&event["data"]["object"], id, customer_id, livemode)
}
impl StripeBillingService<PostgresBillingStore> {
    /// Authenticated GET evidence only, not verification of an original webhook HMAC.
    /// Acknowledges an already-satisfied paused effect; no subscription/provider write
    /// or replay. Stripe provides no pause epoch token, so this does not prove an
    /// uninterrupted pause or that the original callback caused local active=false.
    /// Managed transports use one signed, generation-scoped paired read.
    /// Stripe event retrieval supports the last 30 days:
    /// <https://docs.stripe.com/api/events/retrieve>.
    /// # Errors
    /// Requires exact pending event, trusted binding/customer/local relation, current
    /// provider paused status, local inactive state and a five-second context window.
    /// Timeout/cancellation/COMMIT failures may be ambiguous; query the receipt.
    pub async fn acknowledge_already_paused_effect(
        &self,
        pending: &PendingWebhook,
        scope: &StripeBindingScope,
        public_id: Uuid,
    ) -> BlueprintResult<PausedEffectAcknowledgement> {
        if pending.provider != "stripe" || pending.event_type != "customer.subscription.paused" {
            return Err(review());
        }
        strict_id(&Value::String(pending.event_id.clone()), "evt_")?;
        let context = self
            .client()
            .verify_context(scope.account_id(), scope.livemode())
            .await?;
        let binding = self
            .base()
            .store()
            .stripe_binding_by_public_id(scope, BillingKind::Subscription, public_id)
            .await?
            .ok_or_else(review)?;
        context.ensure_current(self.client())?;
        let relation = binding.subscription_ownership().ok_or_else(review)?;
        let (event, current) = self
            .client()
            .retrieve_paused_effect_in_context(&context, &pending.event_id, binding.provider_id())
            .await?;
        event_evidence(
            pending,
            &event,
            binding.provider_id(),
            relation.provider_customer_id(),
            context.livemode(),
            context.connected_account(),
        )?;
        subscription(
            &current,
            binding.provider_id(),
            relation.provider_customer_id(),
            context.livemode(),
        )?;
        let snapshot = self
            .base()
            .store()
            .inspect_paused_subscription_effect(pending, &binding)
            .await?;
        context.ensure_current(self.client())?;
        self.base()
            .store()
            .acknowledge_paused_subscription_effect(snapshot, || {
                context.ensure_current(self.client())
            })
            .await
    }
}
