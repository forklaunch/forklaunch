#![forbid(unsafe_code)]

#[cfg(feature = "postgres-identity")]
mod bound_created_payment_link_reads;
#[cfg(feature = "postgres-identity")]
pub use bound_created_payment_link_reads::BoundPaymentLinkListLimit;
#[cfg(feature = "postgres-identity")]
pub mod bound_creation;
#[cfg(feature = "postgres-identity")]
pub mod bound_payment_link_expiration;
#[cfg(feature = "postgres-identity")]
pub mod bound_reads;
#[cfg(feature = "postgres-reconciliation")]
pub mod checkout_repair;
pub mod client;
mod configured_cached_reads;
#[cfg(feature = "http")]
pub mod http;
#[cfg(feature = "postgres-identity")]
pub mod managed_creation;
#[cfg(feature = "postgres-reconciliation")]
pub mod paused_effect;
pub mod services;
pub mod synchronization;

use forklaunch_billing_interface::{BillingService, CheckoutSession, Customer, Subscription};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::domain::Page;
use forklaunch_core::domain::PageRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeBillingConfig {
    pub webhook_secret_env: String,
    pub publishable_key_env: String,
    pub secret_key_env: String,
    pub checkout_success_url: String,
    pub checkout_cancel_url: String,
}

impl StripeBillingConfig {
    #[must_use]
    pub fn new(webhook_secret_env: impl Into<String>) -> Self {
        Self {
            webhook_secret_env: webhook_secret_env.into(),
            publishable_key_env: "STRIPE_PUBLISHABLE_KEY".to_owned(),
            secret_key_env: "STRIPE_SECRET_KEY".to_owned(),
            checkout_success_url: "http://localhost:3000/billing/success".to_owned(),
            checkout_cancel_url: "http://localhost:3000/billing/cancel".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeWebhookEvent {
    pub id: String,
    pub event_type: String,
    pub object_id: String,
}

impl StripeWebhookEvent {
    #[must_use]
    pub fn customer_subscription_updated(
        id: impl Into<String>,
        subscription_id: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            event_type: "customer.subscription.updated".to_owned(),
            object_id: subscription_id.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StripeBillingAdapter {
    pub config: StripeBillingConfig,
}

impl StripeBillingAdapter {
    #[must_use]
    pub fn new(config: StripeBillingConfig) -> Self {
        Self { config }
    }
}

// The old synchronous reference interface cannot perform authenticated async
// provider calls. Fail closed instead of returning fabricated checkout sessions.
impl BillingService for StripeBillingAdapter {
    fn get_customer(&self, _id: &str) -> BlueprintResult<Option<Customer>> {
        Err(async_service_required())
    }
    fn list_customers(&self, _request: PageRequest) -> BlueprintResult<Page<Customer>> {
        Err(async_service_required())
    }
    fn list_subscriptions(&self, _customer_id: &str) -> BlueprintResult<Vec<Subscription>> {
        Err(async_service_required())
    }
    fn create_checkout_session(
        &self,
        _customer_id: &str,
        _plan_id: &str,
        _context: &AuditContext,
    ) -> BlueprintResult<CheckoutSession> {
        Err(async_service_required())
    }
}
fn async_service_required() -> BlueprintError {
    BlueprintError::new(
        "billing_async_service_required",
        "Use services::StripeBillingService for authenticated provider operations",
    )
}

/// Stripe webhook signature verification (the `t=...,v1=<hex>` HMAC-SHA256 scheme).
pub mod webhook {
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    /// Computes the hex-encoded Stripe signature for a timestamped payload.
    ///
    /// # Panics
    ///
    /// Panics if the HMAC implementation rejects the key; SHA-256 accepts any key size.
    #[must_use]
    pub fn sign(secret: &[u8], timestamp: &str, payload: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key size");
        mac.update(format!("{timestamp}.{payload}").as_bytes());
        to_hex(&mac.finalize().into_bytes())
    }

    pub use forklaunch_core::http::stripe_webhook::StripeWebhookVerifier;

    /// Verifies the exact raw body with Stripe's 300-second replay window.
    #[must_use]
    pub fn verify(secret: &[u8], payload: &str, signature_header: &str) -> bool {
        let Ok(secret) = std::str::from_utf8(secret) else {
            return false;
        };
        StripeWebhookVerifier::new(secret.to_owned())
            .is_ok_and(|verifier| verifier.verify(payload.as_bytes(), signature_header))
    }

    fn to_hex(bytes: &[u8]) -> String {
        use std::fmt::Write as _;
        bytes.iter().fold(String::new(), |mut acc, byte| {
            let _ = write!(acc, "{byte:02x}");
            acc
        })
    }
}

#[cfg(test)]
mod tests;

#[cfg(feature = "postgres-identity")]
pub mod bound_plan_creation;
#[cfg(feature = "postgres-identity")]
mod bound_plan_reads;

#[cfg(feature = "postgres-identity")]
pub mod customer_registration;

#[cfg(feature = "postgres-identity")]
pub mod bound_subscription_creation;

#[cfg(feature = "postgres-identity")]
pub mod bound_portal_creation;
#[cfg(feature = "postgres-identity")]
pub mod bound_portal_reads;

#[cfg(feature = "postgres-identity")]
pub mod bound_checkout_creation;
