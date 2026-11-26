//! Stripe and PayPal provider callbacks routes; the ecommerce
//! interface declares each operation's contract.
use crate::{api::controllers::webhook, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("handleStripeWebhook", webhook::handle_stripe_webhook(state))
        .route("handlePaypalWebhook", webhook::handle_paypal_webhook(state))
}
