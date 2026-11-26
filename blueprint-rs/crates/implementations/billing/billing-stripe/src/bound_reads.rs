//! Opt-in verified, durable billing reads through explicit public UUID aliases.
use crate::services::StripeBillingService;
use forklaunch_billing_base::services::{
    BillingKind,
    postgres::{PostgresBillingStore, StripeBindingScope},
};
use forklaunch_billing_interface::services::{CheckoutSessionDto, PaymentLinkDto};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use serde_json::Value;
use uuid::Uuid;

impl StripeBillingService<PostgresBillingStore> {
    /// Reads an explicitly provisioned durable payment-link alias under a fresh
    /// direct or managed account/mode observation. Public UUID and storage key remain distinct.
    /// Only the sensitive-read audit is written; amount/status and encrypted local
    /// data remain unchanged. This does not change the default provider-ID service.
    /// Managed reads require the dedicated generation-scoped payment-link endpoint.
    /// # Errors
    /// Rejects unbound/foreign aliases, changed generations, stale context, invalid
    /// provider evidence, and denied durable read audits. Scope is trusted config.
    pub async fn get_bound_payment_link(
        &self,
        scope: &StripeBindingScope,
        public_id: Uuid,
    ) -> BlueprintResult<PaymentLinkDto> {
        let context = self
            .client()
            .verify_context(scope.account_id(), scope.livemode())
            .await?;
        let binding = self
            .base()
            .store()
            .stripe_binding_by_public_id(scope, BillingKind::PaymentLink, public_id)
            .await?
            .ok_or_else(|| {
                BlueprintError::new(
                    "billing_bound_payment_link_mismatch",
                    "Payment link identity does not match",
                )
            })?;
        context.ensure_current(self.client())?;
        let provider = self
            .client()
            .retrieve_payment_link_in_context(&context, binding.provider_id())
            .await?;
        let data = self
            .base()
            .store()
            .read_stripe_bound_record(&binding)
            .await?;
        context.ensure_current(self.client())?;
        let mut value: PaymentLinkDto = serde_json::from_value(data).map_err(|_| {
            BlueprintError::new(
                "billing_bound_payment_link_mismatch",
                "Payment link identity does not match",
            )
        })?;
        value.id = public_id.to_string();
        value.provider_fields = Some(provider);
        Ok(value)
    }

    /// Opt-in durable read by public UUID, not the existing cache-only trait get.
    /// Verifies direct or managed account/mode, then resolves tenant-scoped ownership before
    /// fetching the bound Stripe object. Rechecks binding generation after network
    /// work, audits durable decryption and checks freshness after DB work too.
    /// No provider/local mutation occurs; only the sensitive-read audit is appended.
    /// `scope` must be trusted server configuration, never an HTTP DTO.
    /// # Errors
    /// Rejects unsupported gateways, unbound/foreign UUIDs, changed account/mode, stale
    /// context, replaced aliases, customer mismatch and malformed provider evidence.
    pub async fn get_bound_checkout_session(
        &self,
        scope: &StripeBindingScope,
        public_id: Uuid,
    ) -> BlueprintResult<CheckoutSessionDto> {
        let context = self
            .client()
            .verify_context(scope.account_id(), scope.livemode())
            .await?;
        let binding = self
            .base()
            .store()
            .stripe_binding_by_public_id(scope, BillingKind::CheckoutSession, public_id)
            .await?
            .ok_or_else(mismatch)?;
        context.ensure_current(self.client())?;
        let provider = self
            .client()
            .retrieve_checkout_in_context(&context, binding.provider_id())
            .await?;
        context.ensure_current(self.client())?;
        if provider["id"] != binding.provider_id()
            || provider["object"] != "checkout.session"
            || provider["livemode"].as_bool() != Some(scope.livemode())
            || provider["mode"] != "subscription"
        {
            return Err(mismatch());
        }
        let provider_customer = customer(&provider["customer"])?;
        let data = self
            .base()
            .store()
            .read_stripe_bound_record(&binding)
            .await?;
        context.ensure_current(self.client())?;
        let mut value: CheckoutSessionDto = serde_json::from_value(data).map_err(|_| mismatch())?;
        if value.customer_id != provider_customer {
            return Err(mismatch());
        }
        value.id = public_id.to_string();
        value.provider_fields = Some(provider);
        Ok(value)
    }
    /// Reads a Checkout created by the explicit durable creation profile, keeping
    /// its public Customer UUID while verifying the separate provider relation.
    /// Requires completed actor-owned journal provenance and current exact Customer
    /// body/registration. This direct-only method adds journal SELECT/audit needs;
    /// the legacy bound getter and its grant contract remain unchanged.
    /// # Errors
    /// Rejects missing provenance, changed dependencies, mismatched provider scope
    /// or customer, and expired context after final DB work. No mutation authority.
    pub async fn get_bound_created_checkout_session(
        &self,
        scope: &StripeBindingScope,
        public_id: Uuid,
    ) -> BlueprintResult<CheckoutSessionDto> {
        let version = self.client().direct_creation_version()?;
        let context = self
            .client()
            .verify_context(scope.account_id(), scope.livemode())
            .await?;
        let binding = self
            .base()
            .store()
            .stripe_binding_by_public_id(scope, BillingKind::CheckoutSession, public_id)
            .await?
            .ok_or_else(mismatch)?;
        let provenance = self
            .base()
            .store()
            .checkout_creation_read_snapshot(&binding, version)
            .await?
            .ok_or_else(mismatch)?;
        context.ensure_current(self.client())?;
        let provider = self
            .client()
            .retrieve_checkout_in_context(&context, binding.provider_id())
            .await?;
        context.ensure_current(self.client())?;
        if provider["id"] != binding.provider_id()
            || provider["object"] != "checkout.session"
            || provider["livemode"].as_bool() != Some(scope.livemode())
            || provider["mode"] != "subscription"
        {
            return Err(mismatch());
        }
        let provider_customer = customer(&provider["customer"])?;
        let data = self
            .base()
            .store()
            .read_checkout_creation_snapshot(&provenance)
            .await?;
        context.ensure_current(self.client())?;
        let mut value: CheckoutSessionDto = serde_json::from_value(data).map_err(|_| mismatch())?;
        let expected_customer = provenance.provider_customer_id();
        if expected_customer != provider_customer {
            return Err(mismatch());
        }
        value.id = public_id.to_string();
        value.provider_fields = Some(provider);
        Ok(value)
    }
}
fn customer(value: &Value) -> BlueprintResult<&str> {
    let id = value
        .as_str()
        .or_else(|| {
            (value["object"] == "customer"
                && value
                    .get("deleted")
                    .is_none_or(|deleted| deleted.as_bool() == Some(false)))
            .then(|| value["id"].as_str())
            .flatten()
        })
        .ok_or_else(mismatch)?;
    if !id.starts_with("cus_")
        || id.len() <= 4
        || id.len() > 128
        || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(mismatch());
    }
    Ok(id)
}
fn mismatch() -> BlueprintError {
    BlueprintError::new(
        "billing_bound_checkout_mismatch",
        "Checkout identity or provider evidence does not match",
    )
}
