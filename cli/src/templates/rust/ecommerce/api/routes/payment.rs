//! Provider payments routes; the ecommerce
//! interface declares each operation's contract.
use crate::{api::controllers::payment, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("createPayment", payment::create_payment(state))
        .route("getPayment", payment::get_payment(state))
}
