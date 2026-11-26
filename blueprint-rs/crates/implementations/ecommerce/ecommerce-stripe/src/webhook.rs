//! Backwards-compatible ecommerce adapter for shared raw-body Stripe authentication.
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::http::stripe_webhook;
#[derive(Debug, Clone)]
pub struct StripeWebhookVerifier(stripe_webhook::StripeWebhookVerifier);
impl StripeWebhookVerifier {
    /// # Errors
    /// Rejects missing or control-containing configuration.
    pub fn new(secret: String) -> BlueprintResult<Self> {
        stripe_webhook::StripeWebhookVerifier::new(secret)
            .map(Self)
            .map_err(|_| crate::config_error())
    }
    #[must_use]
    pub fn verify(&self, raw_body: &[u8], header: &str) -> bool {
        self.0.verify(raw_body, header)
    }
    #[must_use]
    pub fn verify_at(&self, raw_body: &[u8], header: &str, now: u64) -> bool {
        self.0.verify_at(raw_body, header, now)
    }
}
