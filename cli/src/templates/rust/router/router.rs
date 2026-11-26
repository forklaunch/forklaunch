use crate::{
    AppState,
    domain::RecordSummary,
    error::Result,
};
use forklaunch_core::contract::RouteContract;
use forklaunch_core::http::{ErrorKind, ErrorResponse, FrameworkResponse, LocalRouter};

pub const ROUTE_NAME: &str = "{{{router_name}}}";

pub use crate::contracts::{{router_module}}::{{response_name}};

pub fn list(state: &AppState) -> Result<{{response_name}}> {
    let records = state.repository().list()?;
    let summary = RecordSummary::new(state.config().name.clone(), records.len());
    Ok({{response_name}} {
        route: ROUTE_NAME.to_owned(),
        summary,
        records,
    })
}

#[must_use]
pub fn path() -> String {
    format!("/{ROUTE_NAME}")
}

impl {{response_name}} {
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("response DTO must serialize")
    }
}

#[must_use]
pub fn contract() -> RouteContract {
    crate::contracts::{{router_module}}::contract()
}

pub fn register(router: &mut LocalRouter, state: AppState) {
    router.register(contract(), move |_request| match list(&state) {
        Ok(response) => Ok(FrameworkResponse::json(200, response.to_json())),
        Err(error) => Ok(ErrorResponse::new(ErrorKind::Internal, error.to_string()).to_response()),
    });
}
