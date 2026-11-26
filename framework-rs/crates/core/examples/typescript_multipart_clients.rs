//! Generate both native clients for the multipart HTTP response fixture.
use forklaunch_core::codegen::{SdkGenerationPlan, SdkTarget};
use forklaunch_core::contract::{
    BodyContract, BodyKind, ContractRegistry, FieldContract, RouteContract, SchemaRef,
};
use forklaunch_core::rust_sdk::{RustSdkOptions, generate_rust_http_sdk_source};
use forklaunch_core::typescript::{TypeScriptHttpSdkOptions, generate_typescript_http_sdk_source};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let output = PathBuf::from(std::env::args().nth(1).expect("output directory"));
    let mut registry = ContractRegistry::new();
    registry.register(
        RouteContract::get(
            "Get Multipart Response",
            "Multipart response",
            "/multipart-response/:mode",
        )
        .with_param(FieldContract::required("mode", "String"))
        .with_response(
            200,
            Some(BodyContract {
                kind: BodyKind::Multipart,
                schema: SchemaRef::named("Form"),
                content_type: Some("multipart/form-data".into()),
            }),
        ),
    );
    let rust = generate_rust_http_sdk_source(
        &SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry),
        &RustSdkOptions::new("accounts-sdk").with_client_struct("AccountsClient"),
    )?;
    let typescript = generate_typescript_http_sdk_source(
        &SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry),
        &TypeScriptHttpSdkOptions::new("accounts"),
    )?;
    std::fs::write(output.join("native-multipart.rs"), rust)?;
    std::fs::write(output.join("native-multipart.ts"), typescript)?;
    Ok(())
}
