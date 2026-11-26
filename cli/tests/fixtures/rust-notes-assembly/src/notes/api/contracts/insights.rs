//! Static typed operation declarations shared by host builds and runtime bindings.
#[framework_core::contract::api]
mod api {
    use super::super::routes::route;
    use framework_core::contract::{
        BodyContract, HttpMethod,
        endpoint::{Endpoint, JsonBody, NoBody},
    };
    #[access(Authenticated)]
    pub fn request_insights() -> Endpoint<NoBody, JsonBody<crate::schemas::InsightRequested>> {
        Endpoint::from_contract(
            route(
                "Request Insights",
                HttpMethod::Post,
                super::super::routes::INSIGHTS,
            )
            .with_response(
                202,
                Some(BodyContract::json_type::<crate::schemas::InsightRequested>()),
            ),
        )
        .expect("valid insight request")
    }
    #[access(Internal)]
    pub fn apply_insight() -> Endpoint<
        JsonBody<crate::schemas::InternalInsightInput>,
        JsonBody<crate::schemas::InsightResult>,
    > {
        Endpoint::from_contract(
            route(
                "Apply Insight",
                HttpMethod::Post,
                super::super::routes::INTERNAL_INSIGHT,
            )
            .with_request_body(BodyContract::json_type::<
                crate::schemas::InternalInsightInput,
            >())
            .with_response(
                200,
                Some(BodyContract::json_type::<crate::schemas::InsightResult>()),
            ),
        )
        .expect("valid internal insight contract")
    }
}
pub use api::*;
