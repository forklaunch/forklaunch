//! Generate a native SDK for the actual-validator presence/null runtime fixture.
use forklaunch_core::codegen::{SdkGenerationPlan, SdkTarget};
use forklaunch_core::contract::{
    BodyContract, ContractRegistry, FieldContract, RouteContract, SchemaRef,
};
use forklaunch_core::rust_sdk::{RustSdkOptions, generate_rust_http_sdk_source};

fn main() -> anyhow::Result<()> {
    let schema = std::fs::read_to_string(std::env::args().nth(1).expect("schema path"))?;
    let body = BodyContract {
        schema: SchemaRef::named("Presence").with_json_schema(schema),
        ..BodyContract::json("Presence")
    };
    let mut registry = ContractRegistry::new();
    registry.register(
        RouteContract::get("Presence Read Presence", "Presence", "/presence/:index")
            .with_param(FieldContract::required("index", "String"))
            .with_response(200, Some(body.clone())),
    );
    registry.register(
        RouteContract::post("Presence Write Presence", "Presence", "/presence")
            .with_request_body(body.clone())
            .with_response(200, Some(body)),
    );
    let plan = SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry);
    let source = generate_rust_http_sdk_source(
        &plan,
        &RustSdkOptions::new("presence-sdk").with_client_struct("PresenceClient"),
    )?;
    std::fs::write(std::env::args().nth(2).expect("output path"), source)?;
    Ok(())
}
