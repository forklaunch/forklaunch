//! Health check routes; the ecommerce
//! interface declares each operation's contract.
use crate::{api::controllers::health, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("getHealth", health::get_health(state))
}
