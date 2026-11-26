use forklaunch_core::contract::{BodyContract, RouteContract};

#[forklaunch_core::contract::schema]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct {{response_name}} {
    pub route: String,
    pub summary: super::RecordSummary,
    pub records: Vec<super::Record>,
}

pub fn contract() -> RouteContract {
    RouteContract::get(
        "List {{{route_title}}}", "List {{{router_name}}} records", "/{{{router_name}}}",
    ).with_response(200, Some(BodyContract::json_type::<{{response_name}}>()))
}
