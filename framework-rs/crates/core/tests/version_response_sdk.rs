#![cfg(all(feature = "build", feature = "typed-schema"))]
use forklaunch_core::{
    codegen::{SdkGenerationPlan, SdkTarget},
    contract::{
        ContractRegistry, HttpMethod,
        endpoint::{Endpoint, Fields, JsonBody, MultipartBody, NoBody, VersionedEndpoint},
    },
};
#[allow(dead_code)]
#[derive(serde::Deserialize, schemars::JsonSchema)]
struct OldQuery {
    #[schemars(regex(pattern = "^(both|first)$"))]
    select: String,
    body: String,
    header: String,
}
#[allow(dead_code)]
#[derive(serde::Deserialize, schemars::JsonSchema)]
struct NewQuery {
    #[schemars(regex(pattern = "^(both|last)$"))]
    select: String,
    body: String,
    header: String,
}
#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct OldOutput {
    #[schemars(regex(pattern = "^old$"))]
    value: String,
}
#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct NewOutput {
    #[schemars(regex(pattern = "^new$"))]
    value: String,
}
#[derive(serde::Serialize, schemars::JsonSchema)]
struct OldHeader {
    #[serde(rename = "x-branch")]
    #[schemars(regex(pattern = "^old$"))]
    value: String,
}
#[derive(serde::Serialize, schemars::JsonSchema)]
struct NewHeader {
    #[serde(rename = "x-branch")]
    #[schemars(regex(pattern = "^new$"))]
    value: String,
}

#[allow(dead_code)]
#[derive(schemars::JsonSchema)]
struct UploadOld {
    #[schemars(regex(pattern = "^(both|first)$"))]
    label: String,
    file: Vec<u8>,
}
#[allow(dead_code)]
#[derive(schemars::JsonSchema)]
struct UploadNew {
    #[schemars(regex(pattern = "^(both|last)$"))]
    label: String,
    file: Vec<u8>,
}

#[test]
#[allow(clippy::too_many_lines)] // One canonical fixture exercises correlated version/media declarations.
fn emit_exact_version_response_clients() {
    let old = Endpoint::<Fields<NoBody, OldQuery>, JsonBody<OldOutput>>::new(
        "Union",
        "Union",
        HttpMethod::Get,
        "/union",
        200,
    )
    .unwrap()
    .with_typed_response_headers::<OldHeader>()
    .unwrap();
    let new = Endpoint::<Fields<NoBody, NewQuery>, JsonBody<NewOutput>>::new(
        "Union",
        "Union",
        HttpMethod::Get,
        "/union",
        200,
    )
    .unwrap()
    .with_typed_response_headers::<NewHeader>()
    .unwrap();
    let endpoint = VersionedEndpoint::new("first", &old)
        .unwrap()
        .with_version("last", &new)
        .unwrap();
    let mut registry = ContractRegistry::new();
    registry.register(endpoint.contract());
    let mut events = endpoint.contract();
    events.operation_name = "Events".into();
    events.path = "/events".into();
    events.openapi_path = "/events".into();
    for response in events.responses.iter_mut().chain(
        events
            .versions
            .values_mut()
            .flat_map(|version| version.responses.iter_mut()),
    ) {
        if response.status_code != 200 {
            continue;
        }
        let body = response.body.as_mut().unwrap();
        body.kind = forklaunch_core::contract::BodyKind::ServerSentEvent;
        body.content_type = Some("text/event-stream".into());
        let mut schema: serde_json::Value =
            serde_json::from_str(body.schema.json_schema.as_ref().unwrap()).unwrap();
        schema["x-forklaunch-sse-framing"] = "payload".into();
        body.schema.json_schema = Some(schema.to_string());
    }
    registry.register(events);
    let upload_old = Endpoint::<MultipartBody<UploadOld>, JsonBody<OldOutput>>::new(
        "Upload",
        "Upload",
        HttpMethod::Post,
        "/upload",
        200,
    )
    .unwrap();
    let upload_new = Endpoint::<MultipartBody<UploadNew>, JsonBody<NewOutput>>::new(
        "Upload",
        "Upload",
        HttpMethod::Post,
        "/upload",
        200,
    )
    .unwrap();
    registry.register(
        VersionedEndpoint::new("first", &upload_old)
            .unwrap()
            .with_version("last", &upload_new)
            .unwrap()
            .contract(),
    );

    let scalar_old = Endpoint::<JsonBody<u8>, JsonBody<OldOutput>>::new(
        "Scalar",
        "Scalar",
        HttpMethod::Post,
        "/scalar",
        200,
    )
    .unwrap();
    let scalar_new = Endpoint::<JsonBody<u8>, JsonBody<NewOutput>>::new(
        "Scalar",
        "Scalar",
        HttpMethod::Post,
        "/scalar",
        200,
    )
    .unwrap();
    let mut scalar = VersionedEndpoint::new("first", &scalar_old)
        .unwrap()
        .with_version("last", &scalar_new)
        .unwrap()
        .contract();
    scalar.options.body_parser.json_strict = false;
    scalar.options.body_parser.json_wire.limit_bytes = 2;
    registry.register(scalar);

    let ts = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry),
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("versions"),
    )
    .unwrap();
    let rust = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry),
        &forklaunch_core::rust_sdk::RustSdkOptions::new("versions"),
    )
    .unwrap();
    let imported = SdkGenerationPlan::from_artifact_json(
        SdkTarget::TypeScript,
        &registry.clone().into_artifact().to_json(),
    )
    .unwrap();
    let imported_ts = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &imported,
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("versions"),
    )
    .unwrap();
    assert_eq!(
        ts, imported_ts,
        "artifact and native version metadata must agree"
    );
    let mut injected: serde_json::Value =
        serde_json::from_str(&registry.clone().into_artifact().to_json()).unwrap();
    injected["routes"][0]["x-forklaunch-version-branches"] = serde_json::json!([]);
    assert!(
        SdkGenerationPlan::from_artifact_json(SdkTarget::TypeScript, &injected.to_string())
            .unwrap_err()
            .to_string()
            .contains("compiler-owned")
    );
    let mut unsupported = SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry);
    unsupported.operations[0].version_branches[0].request_body_kind =
        Some(forklaunch_core::contract::BodyKind::ServerSentEvent);
    assert!(
        forklaunch_core::typescript::generate_typescript_http_sdk_source(
            &unsupported,
            &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("versions")
        )
        .unwrap_err()
        .to_string()
        .contains("Portable version response matching")
    );
    let mut custom_parser = SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry);
    custom_parser.operations[0].version_branches[0].request_parser_options =
        Some("{\"jsonDefaultCharset\":\"utf-16\"}".into());
    assert!(
        forklaunch_core::typescript::generate_typescript_http_sdk_source(
            &custom_parser,
            &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("versions")
        )
        .unwrap_err()
        .to_string()
        .contains("UTF-8 defaults")
    );
    assert!(ts.contains("versionMetadata: JSON.parse"));
    assert!(rust.contains("execute_versioned"));
    if let Ok(directory) = std::env::var("FORKLAUNCH_VERSION_SDK_DIR") {
        let directory = std::path::Path::new(&directory);
        std::fs::write(directory.join("client.ts"), ts).unwrap();
        std::fs::write(directory.join("client.rs"), rust).unwrap();
    }
}
