use crate::AppState;
pub use crate::contracts::health::{HealthResponse, contract};
use forklaunch_core::http::{FrameworkResponse, LocalRouter};

pub const ROUTE_NAME: &str = "health";

pub fn handle(state: &AppState) -> HealthResponse {
    HealthResponse {
        service: state.config().name.clone(),
        status: "ok".to_string(),
        record_count: state.repository().count().unwrap_or_default(),
        routes: crate::route_names().into_iter().map(str::to_owned).collect(),
    }
}

impl HealthResponse {
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("response DTO must serialize")
    }
}

pub fn register(router: &mut LocalRouter, state: AppState) {
    router.register(contract(), move |_request| {
        Ok(FrameworkResponse::json(200, handle(&state).to_json()))
    });
}
