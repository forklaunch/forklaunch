pub mod health;
use forklaunch_core::contract::{ContractRegistry, RouteContract, ServiceContract, assembly::BindingDeclaration};

pub fn routes() -> Vec<RouteContract> { vec![health::contract()] }
pub fn registry() -> ContractRegistry {
    let mut registry = ContractRegistry::for_service(
        ServiceContract::new("{{{name}}}", env!("CARGO_PKG_VERSION")),
    );
    for route in routes() { registry.register(route); }
    registry
}

pub fn binding_manifest() -> Result<Vec<BindingDeclaration>, String> {
    health::binding_manifest()
}
