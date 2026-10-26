//! Native generation for the versioned HTTP/MCP fixture.
use anyhow::Context;
use forklaunch_core::codegen::{SdkGenerationPlan, SdkTarget};
use forklaunch_core::contract::{
    BodyContract, ContractRegistry, RequestContract, ResponseContract, RouteContract,
    VersionedContract,
};
use serde_json::Value;

fn registry(artifact: &Value) -> anyhow::Result<ContractRegistry> {
    let mut registry = ContractRegistry::new();
    for route in artifact["routes"]
        .as_array()
        .context("routes")?
        .iter()
        .filter(|route| route["path"] == "/versions" || route["path"] == "/versioned-transform")
    {
        let mut contract = RouteContract::post(
            route["name"].as_str().context("name")?,
            route["summary"].as_str().context("summary")?,
            route["path"].as_str().context("path")?,
        );
        for version in route["versions"].as_array().context("versions")? {
            let mut input = BodyContract::json(
                version["request"]["body"]["schema"]["name"]
                    .as_str()
                    .context("input name")?,
            );
            input.schema = input
                .schema
                .with_json_schema(version["request"]["body"]["schema"]["jsonSchema"].to_string());
            let mut output = BodyContract::json(
                version["responses"][0]["body"]["schema"]["name"]
                    .as_str()
                    .context("output name")?,
            );
            output.schema = output.schema.with_json_schema(
                version["responses"][0]["body"]["schema"]["jsonSchema"].to_string(),
            );
            contract = contract.with_version(VersionedContract {
                version: version["version"].as_str().context("version name")?.into(),
                request: RequestContract::default().with_body(input),
                responses: vec![ResponseContract::new(
                    u16::try_from(
                        version["responses"][0]["statusCode"]
                            .as_u64()
                            .context("response status")?,
                    )?,
                    Some(output),
                )],
            });
        }
        registry.register(contract);
    }
    Ok(registry)
}
fn main() -> anyhow::Result<()> {
    let artifact: Value = serde_json::from_str(&std::fs::read_to_string(
        std::env::args().nth(1).context("artifact path")?,
    )?)?;
    let source = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry(&artifact)?),
        &forklaunch_core::rust_sdk::RustSdkOptions::new("versions-sdk")
            .with_client_struct("VersionsClient"),
    )?;
    std::fs::write(std::env::args().nth(2).context("output path")?, source)?;
    Ok(())
}
