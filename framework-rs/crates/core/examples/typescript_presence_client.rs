//! Generate a typed native TypeScript SDK for the actual-validator fixture.
use forklaunch_core::codegen::{SdkGenerationPlan, SdkTarget};
use forklaunch_core::contract::{
    BodyContract, ContractRegistry, FieldContract, RouteContract, SchemaRef,
};
use forklaunch_core::typescript::{TypeScriptHttpSdkOptions, generate_typescript_http_sdk_source};

fn main() -> anyhow::Result<()> {
    let schema = std::fs::read_to_string(std::env::args().nth(1).expect("schema path"))?;
    let body = BodyContract {
        schema: SchemaRef::named("Presence").with_json_schema(schema),
        ..BodyContract::json("Presence")
    };
    let mut registry = ContractRegistry::new();
    registry.register(
        RouteContract::get("Read Presence", "Presence", "/presence/:index")
            .with_param(FieldContract::required("index", "String"))
            .with_response(200, Some(body.clone())),
    );
    registry.register(
        RouteContract::post("Write Presence", "Presence", "/presence")
            .with_request_body(body.clone())
            .with_response(200, Some(body)),
    );
    let source = generate_typescript_http_sdk_source(
        &SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry),
        &TypeScriptHttpSdkOptions::new("presence"),
    )?;
    std::fs::write(std::env::args().nth(2).expect("output path"), source)?;
    Ok(())
}
