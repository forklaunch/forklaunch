//! Generate JSON request types for the compiled transport and compile-fail fixture.
use forklaunch_core::codegen::{SdkGenerationPlan, SdkTarget};
use forklaunch_core::contract::{BodyContract, ContractRegistry, RouteContract, SchemaRef};
use forklaunch_core::rust_sdk::{RustSdkOptions, generate_rust_http_sdk_source};

fn main() -> anyhow::Result<()> {
    let mut registry = ContractRegistry::new();
    for (name, schema) in [
        (
            "Create",
            r##"{"type":"object","additionalProperties":false,"properties":{"name":{"type":"string"},"amount":{"type":"number"},"nested":{"$ref":"#/$defs/Child"},"note":{"type":["string","null"]}},"required":["name","nested","note"],"$defs":{"Child":{"type":"object","additionalProperties":false,"properties":{"enabled":{"type":"boolean"}},"required":["enabled"]}}}"##,
        ),
        ("List", r#"{"type":"array","items":{"type":"integer"}}"#),
        ("Text", r#"{"type":"string"}"#),
        ("Null", r#"{"type":"null"}"#),
        ("Dynamic", "{}"),
    ] {
        registry.register(
            RouteContract::post(name, "Typed JSON", format!("/{name}"))
                .with_request_body(BodyContract {
                    schema: SchemaRef::named("Input").with_json_schema(schema),
                    ..BodyContract::json("Input")
                })
                .with_response(200, Some(BodyContract::json("String"))),
        );
    }
    let source = generate_rust_http_sdk_source(
        &SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry),
        &RustSdkOptions::new("typed-request-proof").with_client_struct("Client"),
    )?;
    std::fs::write(std::env::args().nth(1).expect("output path"), source)?;
    Ok(())
}
