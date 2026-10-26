//! Subject data export and erasure routes; the CAC
//! interface declares each operation's contract.
use crate::{api::controllers::compliance, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("exportUserData", compliance::export_user_data(state))
        .route("eraseUserData", compliance::erase_user_data(state))
}
