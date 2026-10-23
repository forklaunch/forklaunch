//! Application-owned routes, command hooks and state registration.
use crate::api::routes::{attachment_routes, insight_routes, notes_routes, realtime_routes};
use crate::registrations::AppState;
use axum::{Router, http::StatusCode, routing::get};
use framework::native::{NativeApplication, ApplicationResult};

fn http_router(state: AppState) -> ApplicationResult<Router> {
    let router = Router::new()
        .route("/health", get(|| async { StatusCode::NO_CONTENT }))
        .merge(notes_routes())
        .merge(attachment_routes())
        .merge(insight_routes())
        .merge(realtime_routes(state.shutdown.clone()))
        .with_state(state);
    Ok(framework::native::with_openapi(router, &fieldnotes_contracts::registry(), "/openapi.json")?)
}


pub fn http(state: AppState) -> ApplicationResult<NativeApplication> {
    let origin = state.public_origin.to_string();
    let telemetry = state.telemetry.clone();
    NativeApplication::new(http_router(state)?, fieldnotes_contracts::registry(), "Fieldnotes", env!("CARGO_PKG_VERSION"), &origin, telemetry)
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|argument| argument == "--init-objects") {
        crate::initialize_attachment_bucket().await?;
        return Ok(());
    }
    if std::env::args().nth(1).as_deref()==Some("--migrate") { return crate::migrations::execute("--migrate-only").await; }
    if let Some(command) = std::env::args().nth(1)
        && matches!(
            command.as_str(),
            "--migrate-only" | "--migrate-status" | "--migrate-baseline"
        )
    {
        return crate::migrations::execute(&command).await;
    }
    let config = crate::config::Config::from_env()?;
    let (state, address) = crate::registrations::register(config).await?;
    let listener = tokio::net::TcpListener::bind(&address).await?;
    let stopping = state.shutdown.clone();
    crate::server::serve(state, listener, stopping.signal()).await
}

/// Application export retaining the reviewed in-process router API.
pub fn router(state:AppState)->Router { http(state).expect("Invalid application origin").router().expect("Invalid contracts") }
