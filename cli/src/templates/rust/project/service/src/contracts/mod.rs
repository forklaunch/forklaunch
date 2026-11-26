//! Shared by the host build script and the running service.
{{#modules}}pub mod {{module}};
{{/modules}}

pub mod models;
pub use models::{Record, RecordSummary};
use forklaunch_core::contract::{ContractRegistry, RouteContract, ServiceContract};

pub fn routes() -> Vec<RouteContract> {
    vec![{{#modules}}{{module}}::contract(),{{/modules}}]
}

pub fn registry() -> ContractRegistry {
    let mut registry = ContractRegistry::for_service(
        ServiceContract::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
    );
    for route in routes() { registry.register(route); }
    registry
}
