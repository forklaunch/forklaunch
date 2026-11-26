//! Composes the HTTP router from registered state; no process startup.
use crate::{api::routes, registrations::AppState};
use forklaunch_axum::ForkLaunchRouter;

pub fn http(state: &AppState) -> ForkLaunchRouter {
    routes::router(state)
}
