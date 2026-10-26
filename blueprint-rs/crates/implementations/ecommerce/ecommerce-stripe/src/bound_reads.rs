//! Opt-in scoped Stripe inspection; no provider or local payment mutations.
use crate::{StripePaymentService, context::mismatch};
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_base::postgres::{PaymentProviderScope, PostgresEcommerceStore};
use serde::Serialize;

/// A read-only observation. No secret, address, customer data or financial action
/// capability is included. Status alone does not prove captured/cancelled funds.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundPaymentIntentObservation {
    pub provider_ref: String,
    pub status: String,
    pub amount_cents: i64,
    pub currency: String,
    pub livemode: bool,
}
impl StripePaymentService<PostgresEcommerceStore> {
    /// Verifies direct account/mode, reads the administratively bound intent and
    /// rechecks the entire durable binding after I/O. No new ownership is inferred.
    /// No cancelled-order repair or provider-reference clearing is performed.
    /// # Errors
    /// Rejects Connect, non-Stripe scope, missing/changed bindings, malformed or
    /// mismatched provider data and context expiry, including after database waits.
    pub async fn inspect_bound_payment_intent(
        &self,
        scope: &PaymentProviderScope,
        payment_id: &str,
    ) -> BlueprintResult<BoundPaymentIntentObservation> {
        self.inspect_bound_payment_intent_profile(scope, payment_id, false)
            .await
    }
    /// Explicit Connect counterpart: verifies the same configured account on every
    /// provider GET and retains all durable binding/freshness checks.
    /// # Errors
    /// Rejects missing/mismatched Connect configuration, scope, evidence or binding.
    pub async fn inspect_bound_connected_payment_intent(
        &self,
        scope: &PaymentProviderScope,
        payment_id: &str,
    ) -> BlueprintResult<BoundPaymentIntentObservation> {
        self.inspect_bound_payment_intent_profile(scope, payment_id, true)
            .await
    }
    async fn inspect_bound_payment_intent_profile(
        &self,
        scope: &PaymentProviderScope,
        payment_id: &str,
        connected: bool,
    ) -> BlueprintResult<BoundPaymentIntentObservation> {
        if self.connect.is_some() != connected || scope.provider() != "stripe" {
            return Err(mismatch());
        }
        let context = self
            .client
            .verify_service_context(scope.merchant_id(), scope.livemode(), self.connect.as_ref())
            .await?;
        let binding = self
            .base
            .store()
            .payment_provider_binding(scope, payment_id)
            .await?;
        context.ensure_current(&self.client, self.connect.as_ref())?;
        let intent = context
            .retrieve(&self.client, self.connect.as_ref(), binding.provider_ref())
            .await?;
        let currency = intent["currency"].as_str().ok_or_else(mismatch)?;
        // Stripe currency is canonical lowercase; the administrative binding stores uppercase.
        if currency.len() != 3
            || !currency.bytes().all(|b| b.is_ascii_lowercase())
            || currency.to_ascii_uppercase() != binding.currency()
            || intent["amount"].as_i64() != Some(binding.amount_cents())
            || intent["metadata"]["orderId"].as_str() != Some(binding.order_id())
        {
            return Err(mismatch());
        }
        let status = intent["status"]
            .as_str()
            .filter(|value| {
                matches!(
                    *value,
                    "requires_payment_method"
                        | "requires_confirmation"
                        | "requires_action"
                        | "processing"
                        | "requires_capture"
                        | "canceled"
                        | "succeeded"
                )
            })
            .ok_or_else(mismatch)?;
        let current = self
            .base
            .store()
            .payment_provider_binding(scope, payment_id)
            .await?;
        context.ensure_current(&self.client, self.connect.as_ref())?;
        if current != binding {
            return Err(mismatch());
        }
        Ok(BoundPaymentIntentObservation {
            provider_ref: binding.provider_ref().into(),
            status: status.into(),
            amount_cents: binding.amount_cents(),
            currency: currency.into(),
            livemode: context.livemode(),
        })
    }
}
