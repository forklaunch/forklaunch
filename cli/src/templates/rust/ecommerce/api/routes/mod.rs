//! Binds the module's contracts to its controllers. Routers added with
//! `forklaunch init router` live in `extensions`.
pub mod cart;
pub mod catalog_import;
pub mod checkout;
pub mod extensions;
pub mod health;
pub mod inventory;
pub mod order;
pub mod payment;
pub mod product;
pub mod variant;
pub mod webhook;

use crate::registrations::AppState;
use forklaunch_axum::ForkLaunchRouter;

pub fn router(state: &AppState) -> ForkLaunchRouter {
    let routes = state.ecommerce.routes();
    let routes = webhook::mount(routes, state);
    let routes = checkout::mount(routes, state);
    let routes = catalog_import::mount(routes, state);
    let routes = product::mount(routes, state);
    let routes = variant::mount(routes, state);
    let routes = inventory::mount(routes, state);
    let routes = cart::mount(routes, state);
    let routes = order::mount(routes, state);
    let routes = payment::mount(routes, state);
    let routes = health::mount(routes, state);
    extensions::register_router(routes.finish())
}
