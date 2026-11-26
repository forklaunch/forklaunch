//! Scoped, explicit operator bookkeeping from an authenticated voided order.
use crate::{
    PaypalClient, PaypalPaymentService,
    context::{PaypalEnvironment, mismatch},
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_base::{
    cancellation::VerifiedCancellation,
    postgres::{PaymentProviderScope, PostgresEcommerceStore, ScopedPaypalCancellationOutcome},
};
use uuid::Uuid;

impl PaypalPaymentService<PostgresEcommerceStore, PaypalClient> {
    /// Repairs only a nonsucceeded payment on an already-cancelled local order.
    /// Requires an exact authenticated CAPTURE order/payee and explicit empty
    /// capture, authorization and refund arrays. Missing histories remain manual.
    /// Never voids/refunds/captures money, clears a reference or transitions orders.
    /// Retain `receipt_id` before calling: ambiguous COMMIT requires inspection,
    /// and a missing receipt never grants permission to retry provider operations.
    /// The environment is trusted configuration, not a response-attested mode.
    /// # Errors
    /// Rejects invalid operator scope, stale identities/evidence, nonempty or
    /// missing histories, capture audits, failed persistence and unknown COMMIT.
    pub async fn reconcile_bound_voided_order_payment(
        &self,
        scope: &PaymentProviderScope,
        payment_id: &str,
        actor: AuditContext,
        receipt_id: Uuid,
    ) -> BlueprintResult<ScopedPaypalCancellationOutcome> {
        if scope.provider() != "paypal"
            || receipt_id.is_nil()
            || actor
                .actor_id
                .as_deref()
                .is_none_or(|s| s.trim().is_empty())
            || actor.correlation_id.trim().is_empty()
        {
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
        let snapshot = self
            .base
            .store()
            .cancelled_paypal_order_payment_snapshot(&binding)
            .await?;
        let context = self
            .client
            .verify_order_context(environment, scope.merchant_id(), binding.provider_ref())
            .await?;
        context.require_voided_empty_history()?;
        let observation = context.observation();
        if context.reference_id() != binding.order_id()
            || observation.amount_cents != binding.amount_cents()
            || observation.currency != binding.currency()
        {
            return Err(mismatch());
        }
        let evidence = VerifiedCancellation {
            provider: "paypal".into(),
            provider_ref: observation.provider_ref.clone(),
            amount_cents: observation.amount_cents,
            currency: observation.currency.clone(),
            canceled_at: None,
        };
        self.base
            .store()
            .record_scoped_paypal_order_cancellation(snapshot, evidence, actor, receipt_id, || {
                context.ensure_current(&self.client)
            })
            .await
    }
}
