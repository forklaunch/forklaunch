//! Explicit `PayPal` resource-scoped inspection; no local/payment mutations.
use crate::{
    PaypalClient, PaypalPaymentService,
    context::{BoundPaypalOrderObservation, PaypalEnvironment, mismatch},
};
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_base::postgres::{PaymentProviderScope, PostgresEcommerceStore};
impl PaypalPaymentService<PostgresEcommerceStore, PaypalClient> {
    /// Inspects the administratively bound order and requires authenticated exact
    /// payee evidence. Rechecks full tenant binding/current local relation after
    /// I/O, then checks freshness after all DB waits. No ownership is inferred
    /// from mutable provider fields, email, OAuth `app_id` or a status label.
    /// # Errors
    /// Rejects wrong/unprofiled environment before provider I/O, changed binding,
    /// wrong provider/merchant/resource/amount/reference and expired evidence.
    pub async fn inspect_bound_order(
        &self,
        scope: &PaymentProviderScope,
        payment_id: &str,
    ) -> BlueprintResult<BoundPaypalOrderObservation> {
        if scope.provider() != "paypal" {
            return Err(mismatch());
        }
        let environment = if scope.livemode() {
            PaypalEnvironment::Live
        } else {
            PaypalEnvironment::Sandbox
        };
        self.client.require_environment(environment)?;
        let binding = self
            .base
            .store()
            .payment_provider_binding(scope, payment_id)
            .await?;
        let context = self
            .client
            .verify_order_context(environment, scope.merchant_id(), binding.provider_ref())
            .await?;
        if context.reference_id() != binding.order_id()
            || context.observation().amount_cents != binding.amount_cents()
            || context.observation().currency != binding.currency()
        {
            return Err(mismatch());
        }
        let current = self
            .base
            .store()
            .payment_provider_binding(scope, payment_id)
            .await?;
        context.ensure_current(&self.client)?;
        if current != binding {
            return Err(mismatch());
        }
        Ok(context.into_observation())
    }
}
