//! Subscriptions routes; the billing
//! interface declares each operation's contract.
use crate::{api::controllers::subscription, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("createSubscription", subscription::create_subscription(state))
        .route("listSubscriptions", subscription::list_subscriptions(state))
        .route("getSubscription", subscription::get_subscription(state))
        .route("getUserSubscription", subscription::get_user_subscription(state))
        .route("getOrganizationSubscription", subscription::get_organization_subscription(state))
        .route("updateSubscription", subscription::update_subscription(state))
        .route("deleteSubscription", subscription::delete_subscription(state))
        .route("cancelSubscription", subscription::cancel_subscription(state))
        .route("resumeSubscription", subscription::resume_subscription(state))
}
