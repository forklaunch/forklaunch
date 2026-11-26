//! Generate a typed native TypeScript client for the bounded SSE HTTP/MCP runtime fixture.
use forklaunch_core::codegen::{SdkGenerationPlan, SdkTarget};
use forklaunch_core::contract::{
    BodyContract, BodyKind, ContractRegistry, FieldContract, RouteContract, SchemaRef,
};
use forklaunch_core::typescript::{TypeScriptHttpSdkOptions, generate_typescript_http_sdk_source};

fn main() -> anyhow::Result<()> {
    let mut registry = ContractRegistry::new();
    registry.register(RouteContract::get("Watch Events", "Bounded event stream", "/events/:mode")
        .with_param(FieldContract::required("mode", "String"))
        .with_response(200, Some(BodyContract {
            kind: BodyKind::ServerSentEvent,
            schema: SchemaRef::named("StreamEvent").with_json_schema(r#"{"type":"object","required":["data"],"properties":{"data":{},"id":{"type":"string"}}}"#),
            content_type: Some("text/event-stream".into()),
        })));
    let source = generate_typescript_http_sdk_source(
        &SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry),
        &TypeScriptHttpSdkOptions::new("streams")
            .with_fetch_import(std::env::args().any(|arg| arg == "--import-fetch")),
    )?;
    std::fs::write(std::env::args().nth(1).expect("output path"), source)?;
    Ok(())
}
