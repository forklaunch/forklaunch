//! Reusable application composition from explicit dependencies; no process startup.
use forklaunch_axum::native::Router;
use crate::{api::routes, registrations::AppState};
use forklaunch_axum::native::{NativeApplication, ApplicationResult};
use forklaunch_axum::native;

pub fn http_router(state: AppState) -> ApplicationResult<Router> {
    let registry = contracts::registry();
    let router = native::with_openapi(
        routes::router()?, &registry, &state.config.docs_path, &state.config.version,
    )?.with_state(state);
    Ok(router)
}


pub fn http(state: AppState) -> ApplicationResult<NativeApplication> {
    let origin = state.config.origin();
    let cors_origins = state.config.cors_origins.clone();
    let telemetry = state.telemetry.clone();
    NativeApplication::new(http_router(state)?, contracts::registry(), "{{{name}}}", env!("CARGO_PKG_VERSION"), &origin, telemetry)?
        .with_cors_origins(cors_origins)
}

