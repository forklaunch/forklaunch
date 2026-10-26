//! Payment links routes; the billing
//! interface declares each operation's contract.
use crate::{api::controllers::payment_link, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("createPaymentLink", payment_link::create_payment_link(state))
        .route("getPaymentLink", payment_link::get_payment_link(state))
        .route("updatePaymentLink", payment_link::update_payment_link(state))
        .route("expirePaymentLink", payment_link::expire_payment_link(state))
        .route("handlePaymentSuccess", payment_link::handle_payment_success(state))
        .route("handlePaymentFailure", payment_link::handle_payment_failure(state))
        .route("listPaymentLinks", payment_link::list_payment_links(state))
}
