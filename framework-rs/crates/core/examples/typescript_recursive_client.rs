//! Generate a typed native TypeScript client for the recursive contract HTTP/MCP fixture.
use anyhow::Context;
use forklaunch_core::codegen::{SdkGenerationPlan, SdkTarget};
use forklaunch_core::contract::{
    BodyContract, ContractRegistry, HttpMethod, RouteContract, SchemaRef,
};
use forklaunch_core::typescript::{TypeScriptHttpSdkOptions, generate_typescript_http_sdk_source};
use serde_json::Value;

fn main() -> anyhow::Result<()> {
    let artifact: Value = serde_json::from_str(&std::fs::read_to_string(
        std::env::args().nth(1).context("contract path")?,
    )?)?;
    let mut registry = ContractRegistry::new();
    for (name, path, method) in [
        ("Read Tree", "/trees", HttpMethod::Get),
        ("Write Tree", "/trees", HttpMethod::Post),
        ("Read Forest", "/forests", HttpMethod::Get),
        ("Read Choice", "/choices", HttpMethod::Get),
        ("Read Mutual", "/mutual", HttpMethod::Get),
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
    let source = generate_typescript_http_sdk_source(
        &SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry),
        &TypeScriptHttpSdkOptions::new("trees"),
    )?;
    std::fs::write(std::env::args().nth(2).context("output path")?, source)?;
    Ok(())
}
