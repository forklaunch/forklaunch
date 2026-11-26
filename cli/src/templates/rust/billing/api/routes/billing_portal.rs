//! Billing portal sessions routes; the billing
//! interface declares each operation's contract.
use crate::{api::controllers::billing_portal, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("createBillingPortalSession", billing_portal::create_billing_portal_session(state))
        .route("getBillingPortalSession", billing_portal::get_billing_portal_session(state))
        .route("updateBillingPortalSession", billing_portal::update_billing_portal_session(state))
        .route("expireBillingPortalSession", billing_portal::expire_billing_portal_session(state))
}
