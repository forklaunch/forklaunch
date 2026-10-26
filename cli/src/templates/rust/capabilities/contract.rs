// BEGIN FORKLAUNCH MANAGED CAPABILITIES
//! Typed callback contract; runtime verification authenticates original wire bytes.
use forklaunch_core::contract::{AccessLevel, ContractRegistry, FieldContract, HttpMethod, RouteContract, endpoint::{Endpoint, JsonBody, TextBody}, BodyContract, JsonValue};
#[forklaunch_core::contract::schema]
#[serde(rename_all = "camelCase")]
pub struct PlatformEvent {
    pub id: String,
    pub feature: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub occurred_at: Option<String>,
    pub data: Option<JsonValue>,
}
pub fn endpoint() -> Endpoint<JsonBody<PlatformEvent>, TextBody> {
    let mut route = Endpoint::<JsonBody<PlatformEvent>, TextBody>::new("ReceivePlatformEvent", "Receives an instance-signed platform event", HttpMethod::Post, "/platform-events/:feature", 200)
        .expect("valid callback endpoint").with_access(AccessLevel::Internal).into_contract()
        .with_param(FieldContract { name: "feature".into(), schema: BodyContract::json_type::<String>().schema, required: true });
    route.options.mcp = false;
    Endpoint::from_contract(route).expect("valid callback path")
}
pub fn contracts() -> Vec<RouteContract> { vec![endpoint().contract()] }
pub fn register_contracts(registry: &mut ContractRegistry) {
    if !registry.routes().iter().any(|route| route.path == "/platform-events/:feature") {
        for contract in contracts() { registry.register(contract); }
    }
}
// END FORKLAUNCH MANAGED CAPABILITIES
