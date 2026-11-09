//! Binds the module's contracts to its controllers. Routers added with
//! `forklaunch init router` live in `extensions`.
pub mod billing_portal;
pub mod bound;
pub mod checkout_session;
pub mod compliance;
pub mod extensions;
pub mod health;
pub mod payment_link;
pub mod plan;
pub mod subscription;
{{#stripe}}pub mod webhook;
{{/stripe}}
use crate::registrations::AppState;
use forklaunch_axum::ForkLaunchRouter;

pub fn router(state: &AppState) -> ForkLaunchRouter {
    let routes = state.billing.routes();
    let routes = health::mount(routes, state);
    let routes = plan::mount(routes, state);
    let routes = subscription::mount(routes, state);
    let routes = checkout_session::mount(routes, state);
    let routes = payment_link::mount(routes, state);
    let routes = billing_portal::mount(routes, state);
    let routes = compliance::mount(routes, state);
{{#stripe}}    let routes = webhook::mount(routes, state);
{{/stripe}}    let routes = bound::mount(routes);
    extensions::register_router(routes.finish())
}
