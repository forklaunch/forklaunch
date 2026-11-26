//! Explicit trusted association + authenticated provider existence verification.
//! A billing Customer is owned locally; its Party may live in another service.
use crate::services::StripeBillingService;
use forklaunch_billing_base::services::postgres::{
    DurableStripeCustomerBinding, PostgresBillingStore, StripeBindingScope, TrustedCustomerParty,
};
use forklaunch_core::domain::BlueprintResult;

/// Historical association output, not authorization to charge/create subscriptions.
pub enum CustomerRegistrationOutcome {
    Registered(DurableStripeCustomerBinding),
    /// COMMIT succeeded but the evidence expired before the method returned.
    /// The association exists; fresh verification is required for later operations.
    RecordedEvidenceNoLongerCurrent(DurableStripeCustomerBinding),
}
impl StripeBillingService<PostgresBillingStore> {
    /// Direct fixed GET only; no provider writes. The server caller separately
    /// authorizes the external Party association/existence. Email, metadata and
    /// mutable DTO provider references never establish that authority. A local
    /// encrypted Customer snapshot is rechecked under lock before publication.
    /// No default route or generic `CustomerService` dispatch selects this method.
    /// # Errors
    /// Denies unsupported transport, changed local record, stale/wrong provider
    /// context, deleted/malformed Customer or audit failure. A COMMIT error can
    /// mean publication succeeded: inspect the scoped binding rather than assume
    /// rollback. Provider state may change after the last authenticated GET.
    pub async fn register_verified_stripe_customer(
        &self,
        scope: &StripeBindingScope,
        party: &TrustedCustomerParty,
        provider_customer_id: &str,
    ) -> BlueprintResult<CustomerRegistrationOutcome> {
        let version = self.client().direct_customer_registration_version()?;
        let snapshot = self
            .base()
            .store()
            .snapshot_customer_registration(scope, party, provider_customer_id, version)
            .await?;
        let context = self
            .client()
            .verify_context(scope.account_id(), scope.livemode())
            .await?;
        self.client()
            .verify_registration_customer(&context, provider_customer_id)
            .await?;
        let binding = self
            .base()
            .store()
            .complete_customer_registration(snapshot, || context.ensure_current(self.client()))
            .await?;
        if context.ensure_current(self.client()).is_err() {
            return Ok(CustomerRegistrationOutcome::RecordedEvidenceNoLongerCurrent(binding));
        }
        Ok(CustomerRegistrationOutcome::Registered(binding))
    }
}
