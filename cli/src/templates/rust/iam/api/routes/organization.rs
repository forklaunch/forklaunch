//! Directory organizations routes; the IAM
//! interface declares each operation's contract.
use crate::{api::controllers::organization, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("createOrganization", organization::create_organization(state))
        .route("getOrganization", organization::get_organization(state))
        .route("updateOrganization", organization::update_organization(state))
        .route("deleteOrganization", organization::delete_organization(state))
}
