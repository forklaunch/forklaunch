//! Orders and their transitions routes; the ecommerce
//! interface declares each operation's contract.
use crate::{api::controllers::order, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("createOrder", order::create_order(state))
        .route("listOrders", order::list_orders(state))
        .route("getOrder", order::get_order(state))
        .route("transitionOrder", order::transition_order(state))
}
