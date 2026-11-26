use forklaunch_core::contract::{BodyContract, RouteContract};

#[forklaunch_core::contract::schema(rename_all = "camelCase")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthResponse {
    pub service: String,
    pub status: String,
    #[field(max = 9007199254740991)]
    pub record_count: usize,
    pub routes: Vec<String>,
}

pub fn contract() -> RouteContract {
    RouteContract::get("Get Health", "Reports service health", "/health")
        .with_response(200, Some(BodyContract::json_type::<HealthResponse>()))
}
