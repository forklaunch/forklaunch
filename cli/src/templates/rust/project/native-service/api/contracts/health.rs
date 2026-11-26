#[forklaunch_core::contract::api]
mod operations {
    use forklaunch_core::contract::{
        HttpMethod,
        endpoint::{Endpoint, JsonBody, NoBody},
    };

    pub fn health() -> Endpoint<NoBody, JsonBody<crate::schemas::HealthResponse>> {
        Endpoint::new(
            "Get Health",
            "Reports service health",
            HttpMethod::Get,
            "/health",
            200,
        )
        .expect("valid health endpoint")
    }
}
pub use operations::{binding_manifest, Bindings, Health, health as endpoint, registry};
use forklaunch_core::contract::RouteContract;
pub fn contract() -> RouteContract {
    endpoint().contract()
}
