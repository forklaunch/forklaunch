//! Generate a native client for the recursive contract HTTP/MCP fixture.
use anyhow::Context;
use forklaunch_core::codegen::{SdkGenerationPlan, SdkTarget};
use forklaunch_core::contract::{
    BodyContract, ContractRegistry, HttpMethod, RouteContract, SchemaRef,
};
use forklaunch_core::rust_sdk::{RustSdkOptions, generate_rust_http_sdk_source};
use serde_json::Value;

fn main() -> anyhow::Result<()> {
    let artifact: Value = serde_json::from_str(&std::fs::read_to_string(
        std::env::args().nth(1).context("contract path")?,
    )?)?;
    let mut registry = ContractRegistry::new();
    for (name, path, method) in [
        ("Trees Read Tree", "/trees", HttpMethod::Get),
        ("Trees Write Tree", "/trees", HttpMethod::Post),
        ("Forests Read Forest", "/forests", HttpMethod::Get),
        ("Choices Read Choice", "/choices", HttpMethod::Get),
        ("Mutual Read Mutual", "/mutual", HttpMethod::Get),
    ] {
        let route = artifact["routes"]
            .as_array()
            .context("routes")?
            .iter()
            .find(|route| route["path"] == path && route["method"] == method.as_str())
            .context("fixture route")?;
        let body = BodyContract {
            schema: SchemaRef::named("Recursive").with_json_schema(
                route["responses"][0]["body"]["schema"]["jsonSchema"].to_string(),
            ),
            ..BodyContract::json("Recursive")
        };
        let mut contract = RouteContract::new(name, "Recursive fixture", method, path)
            .with_response(200, Some(body.clone()));
        if method == HttpMethod::Post {
            contract = contract.with_request_body(body);
        }
        registry.register(contract);
    }
    let source = generate_rust_http_sdk_source(
        &SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry),
        &RustSdkOptions::new("trees-sdk").with_client_struct("TreesClient"),
    )?;
    std::fs::write(std::env::args().nth(2).context("output path")?, source)?;
    Ok(())
}
