// BEGIN FORKLAUNCH MANAGED CAPABILITIES
//! Callback wire contract; runtime policy verifies the original signed bytes.
#[forklaunch_core::contract::api]
pub mod contract {
    use forklaunch_core::contract::{AccessLevel, BodyContract, FieldContract, HttpMethod, JsonValue, endpoint::{Endpoint, JsonBody, TextBody}};
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
    #[access(Internal)]
    pub fn receive_platform_event() -> Endpoint<JsonBody<PlatformEvent>, TextBody> {
        let mut route = Endpoint::<JsonBody<PlatformEvent>, TextBody>::new("ReceivePlatformEvent", "Receives an instance-signed platform event", HttpMethod::Post, "/platform-events/:feature", 200)
            .expect("valid callback endpoint").with_access(AccessLevel::Internal).into_contract()
            .with_param(FieldContract { name: "feature".into(), schema: BodyContract::json_type::<String>().schema, required: true });
        route.options.mcp = false;
        Endpoint::from_contract(route).expect("valid callback path")
    }
}
// END FORKLAUNCH MANAGED CAPABILITIES
