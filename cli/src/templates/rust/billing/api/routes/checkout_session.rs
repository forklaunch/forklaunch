//! Checkout sessions routes; the billing
//! interface declares each operation's contract.
use crate::{api::controllers::checkout_session, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("createCheckoutSession", checkout_session::create_checkout_session(state))
        .route("getCheckoutSession", checkout_session::get_checkout_session(state))
        .route("expireCheckoutSession", checkout_session::expire_checkout_session(state))
        .route("handleCheckoutSuccess", checkout_session::handle_checkout_success(state))
        .route("handleCheckoutFailure", checkout_session::handle_checkout_failure(state))
}
