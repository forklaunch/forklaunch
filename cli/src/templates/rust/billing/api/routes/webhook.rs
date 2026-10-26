{{#stripe}}//! Stripe and managed platform event callbacks routes; the billing
//! interface declares each operation's contract.
use crate::{api::controllers::webhook, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("handlewebhookevent", webhook::handle_webhook_event(state))
        .route("receiveplatformevent", webhook::receive_platform_event(state))
}
{{/stripe}}