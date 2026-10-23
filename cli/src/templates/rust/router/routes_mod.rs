{{#modules}}pub mod {{module}};
{{/modules}}
use crate::AppState;
use forklaunch_core::contract::RouteContract;
use forklaunch_core::http::LocalRouter;

{{#has_health}}pub use health::HealthResponse;
{{/has_health}}
pub fn route_names() -> Vec<&'static str> {
    vec![
{{#modules}}        {{module}}::ROUTE_NAME,
{{/modules}}    ]
}

#[must_use]
pub fn contracts() -> Vec<RouteContract> {
    vec![
{{#modules}}        {{module}}::contract(),
{{/modules}}    ]
}

pub fn register_handlers(router: &mut LocalRouter, state: AppState) {
{{#modules}}    {{module}}::register(router, state.clone());
{{/modules}}}
