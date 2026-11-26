{{#stripe}}//! Stripe and managed platform event callbacks. They are public routes: the
//! crate verifies each raw payload's signature before any billing state changes.
use crate::registrations::AppState;
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::HeaderMap,
    response::Response,
    routing::any,
};
use forklaunch_billing_stripe::http::{BillingHttp, Handler};

/// Applies a verified Stripe event once.
pub fn handle_webhook_event(state: &AppState) -> Handler {
    any(stripe).with_state(state.billing.clone())
}

async fn stripe(State(billing): State<BillingHttp>, headers: HeaderMap, body: Bytes) -> Response {
    billing.stripe_webhook(headers, body).await
}

/// Applies a verified managed platform event for one feature once.
pub fn receive_platform_event(state: &AppState) -> Handler {
    any(platform).with_state(state.billing.clone())
}

async fn platform(
    State(billing): State<BillingHttp>,
    Path(feature): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    billing.platform_event(feature, headers, body).await
}
{{/stripe}}