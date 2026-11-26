//! Checkout with tax and shipping routes; the ecommerce
//! interface declares each operation's contract.
use crate::{api::controllers::checkout, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("checkout", checkout::checkout(state))
}
