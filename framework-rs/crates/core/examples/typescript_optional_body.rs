//! Export one explicit optional contract through metadata and both SDKs.
use forklaunch_core::codegen::{SdkGenerationPlan, SdkTarget};
use forklaunch_core::contract::{
    BodyContract, ContractRegistry, RequestContract, ResponseContract, RouteContract,
    VersionedContract,
};
use serde_json::{Value, json};
fn main() -> anyhow::Result<()> {
    let output = std::path::PathBuf::from(std::env::args().nth(1).expect("output directory"));
    let mut body = BodyContract::json("Input");
    body.schema = body.schema.with_json_schema(
        r#"{"type":"object","required":["flag"],"properties":{"flag":{"type":"string"}}}"#,
    );
    let mut route = RouteContract::post("Presence", "Presence", "/presence");
    for (version, optional) in [("required", false), ("optional", true)] {
        let request = if optional {
            RequestContract::default().with_optional_body(body.clone())
        } else {
            RequestContract::default().with_body(body.clone())
        };
        route = route.with_version(VersionedContract {
            version: version.into(),
            request,
            responses: vec![ResponseContract::new(204, None)],
        });
    }
    assert!(route.for_version("optional").unwrap().request.body_optional);
    assert!(!route.for_version("required").unwrap().request.body_optional);
    let mut registry = ContractRegistry::new();
    registry.register(route);
    let artifact: Value = serde_json::from_str(&registry.clone().into_artifact().to_json())?;
    assert_eq!(
        artifact["routes"][0]["versions"][1]["request"]["bodyOptional"],
        true
    );
    assert!(
        artifact["routes"][0]["versions"][0]["request"]
            .get("bodyOptional")
            .is_none()
    );
    std::fs::write(output.join("contract.json"), artifact.to_string())?;
    let docs = forklaunch_core::openapi::generate_versioned_openapi_json(
        &registry,
        &forklaunch_core::openapi::OpenApiOptions::from_registry(&registry),
    );
    std::fs::write(output.join("openapi.json"), docs)?;
    let plan = SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry);
    assert_eq!(
        plan.operations
            .iter()
            .map(|op| op.request_body_optional)
            .collect::<Vec<_>>(),
        vec![false, true]
    );
    std::fs::write(
        output.join("client.ts"),
        forklaunch_core::typescript::generate_typescript_http_sdk_source(
            &plan,
            &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("optional"),
        )?,
    )?;
    let source = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry),
        &forklaunch_core::rust_sdk::RustSdkOptions::new("OptionalClient"),
    )?;
    // Generated Rust requests carry the typed body model; an optional body stays
    // an `Option`.
    assert!(source.contains(
        "pub struct PresenceVersionOptionalRequest<T = PresenceVersionOptionalRequestBody>"
    ));
    assert!(source.contains("pub body: Option<T>"));
    std::fs::write(output.join("client.rs"), source)?;
    println!("{}", json!({"optional":true,"rustBody":"Option<T>"}));
    Ok(())
}
