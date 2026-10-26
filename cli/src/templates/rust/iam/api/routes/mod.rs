//! Binds the module's contracts to its controllers. Routers added with
//! `forklaunch init router` live in `extensions`.
pub mod browser;
pub mod compliance;
pub mod extensions;
pub mod health;
pub mod organization;
pub mod permission;
pub mod role;
pub mod session;
pub mod user;

use crate::registrations::AppState;
use forklaunch_axum::ForkLaunchRouter;

pub fn router(state: &AppState) -> ForkLaunchRouter {
    let routes = state.iam.routes();
    let routes = health::mount(routes, state);
    let routes = session::mount(routes, state);
    let routes = user::mount(routes, state);
    let routes = role::mount(routes, state);
    let routes = permission::mount(routes, state);
    let routes = organization::mount(routes, state);
    let routes = compliance::mount(routes, state);
    let router = browser::mount(routes.finish(), state);
    extensions::register_router(router)
}
