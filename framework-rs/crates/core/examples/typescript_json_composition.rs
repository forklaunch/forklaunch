//! Generate both transport SDKs for the exact live composition route contracts.
#[path = "../../axum/examples/json_composition/contract.rs"]
mod contract;
use forklaunch_core::codegen::{SdkGenerationPlan, SdkTarget};
use forklaunch_core::contract::{
    BodyContract, BodyKind, ContractRegistry, SchemaRef, ValidationMode,
};
fn main() -> anyhow::Result<()> {
    let output = std::path::PathBuf::from(std::env::args().nth(1).expect("output directory"));
    let mut registry = ContractRegistry::new();
    for (name, mode) in [
        ("none", ValidationMode::None),
        ("warning", ValidationMode::Warning),
        ("error", ValidationMode::Error),
    ] {
        let mut route = contract::route(name, mode, "public");
        if mode == ValidationMode::Error {
            route = route.with_response(
                400,
                Some(BodyContract {
                    kind: BodyKind::Text,
                    schema: SchemaRef::named("String"),
                    content_type: Some("text/plain".into()),
                }),
            );
        }
        registry.register(route);
    }
    std::fs::write(
        output.join("client.ts"),
        forklaunch_core::typescript::generate_typescript_http_sdk_source(
            &SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry),
            &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("composition"),
        )?,
    )?;
    std::fs::write(
        output.join("client.rs"),
        forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
            &SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry),
            &forklaunch_core::rust_sdk::RustSdkOptions::new("composition")
                .with_client_struct("CompositionClient"),
        )?,
    )?;
    Ok(())
}
