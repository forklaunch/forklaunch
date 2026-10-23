//! Bound Checkout, Portal, Plan, Subscription and payment-link capabilities.
//! `BILLING_IDENTITY_PROFILE` and its trusted configuration select them, and
//! `forklaunch-billing-stripe` owns their handlers and contracts.
use crate::registrations::Routes;

pub fn mount(routes: Routes) -> Routes {
    routes.bound()
}
