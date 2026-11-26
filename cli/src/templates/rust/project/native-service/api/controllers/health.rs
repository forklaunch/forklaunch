use forklaunch_axum::native::State;
use forklaunch_axum::native::EndpointError;
use crate::{registrations::AppState, domain::schemas::HealthResponse};

pub async fn health(State(state): State<AppState>) -> Result<HealthResponse, EndpointError> {
    Ok(state.health.report())
}
