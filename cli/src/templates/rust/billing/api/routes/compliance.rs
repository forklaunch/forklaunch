//! Subject data export and erasure routes; the billing
//! interface declares each operation's contract.
use crate::{api::controllers::compliance, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("exportuserdata", compliance::export_user_data(state))
        .route("eraseuserdata", compliance::erase_user_data(state))
}
