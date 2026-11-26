//! Binds the module's contracts to its controllers. Routers added with
//! `forklaunch init router` live in `extensions`.
pub mod code_set;
pub mod compliance;
pub mod extensions;
pub mod health;

use crate::registrations::AppState;
use forklaunch_axum::ForkLaunchRouter;

pub fn router(state: &AppState) -> ForkLaunchRouter {
    let routes = state.cac.routes();
    let routes = code_set::mount(routes, state);
    let routes = compliance::mount(routes, state);
    let routes = health::mount(routes, state);
    extensions::register_router(routes.finish())
}
