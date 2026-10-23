//! Stripe and `PayPal` provider callbacks. They are public routes: the crate
//! verifies each raw payload's provider signature before any state changes.
use crate::registrations::AppState;
use axum::{body::Bytes, extract::State, http::HeaderMap, response::Response, routing::any};
use forklaunch_ecommerce_stripe::http::{EcommerceHttp, Handler};

/// Applies a verified Stripe event once.
pub fn handle_stripe_webhook(state: &AppState) -> Handler {
    any(stripe).with_state(state.ecommerce.clone())
}

async fn stripe(State(ecommerce): State<EcommerceHttp>, headers: HeaderMap, body: Bytes) -> Response {
    ecommerce.stripe_webhook(&headers, &body).await
}

/// Applies a verified `PayPal` event once.
pub fn handle_paypal_webhook(state: &AppState) -> Handler {
    any(paypal).with_state(state.ecommerce.clone())
}

async fn paypal(State(ecommerce): State<EcommerceHttp>, headers: HeaderMap, body: Bytes) -> Response {
    ecommerce.paypal_webhook(&headers, &body).await
}
