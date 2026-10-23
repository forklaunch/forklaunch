#![cfg(all(feature = "typed-schema", feature = "schema-validation"))]
use forklaunch_core::contract::{
    HttpMethod,
    codec::Normalization,
    endpoint::{Endpoint, JsonBody},
    schema,
    schema_validation::compile_checked,
};
use serde_json::json;
#[schema(unknown_fields = "deny")]
struct Title {
    #[field(wire_max_length=12, normalize=Normalization::TrimAscii,min_length=1,max_length=4)]
    title: String,
}
#[test]
fn directional_normalization_preserves_raw_and_value_contracts() {
    let endpoint = Endpoint::<JsonBody<Title>, JsonBody<Title>>::new(
        "normalize",
        "Normalize",
        HttpMethod::Post,
        "/normalize",
        200,
    )
    .unwrap();
    let route = endpoint.contract();
    let input: serde_json::Value = serde_json::from_str(
        route
            .request
            .body
            .as_ref()
            .unwrap()
            .schema
            .json_schema
            .as_ref()
            .unwrap(),
    )
    .unwrap();
    let output: serde_json::Value = serde_json::from_str(
        route.responses[0]
            .body
            .as_ref()
            .unwrap()
            .schema
            .json_schema
            .as_ref()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(input["properties"]["title"]["maxLength"], 12);
    assert_eq!(output["properties"]["title"]["maxLength"], 4);
    assert_eq!(
        forklaunch_core::contract::codec::input_schema(&input),
        input
    );
    let validate_input = compile_checked(&input).unwrap();
    let validate_output = compile_checked(&output).unwrap();
    let mut cases = Vec::new();
    for value in [
        json!("one"),
        json!("  one  "),
        json!("\t\none\r\u{b}"),
        json!("\u{a0}x\u{a0}"),
        json!("🍎🍎🍎🍎"),
        json!(""),
        json!(" \t\n"),
        json!("abcde"),
        json!("      x      "),
        json!("one\n"),
        json!(null),
        json!(42),
    ] {
        let input_value = json!({"title":value});
        let parsed = serde_json::from_value::<Title>(input_value.clone());
        let accepted = parsed.is_ok();
        let normalized = parsed
            .ok()
            .map(|title| serde_json::to_value(title).unwrap());
        if accepted {
            assert!(validate_input(&input_value).is_ok());
        }
        let output_accepted = validate_output(&input_value).is_ok();
        if let Some(text) = value.as_str() {
            assert_eq!(
                serde_json::to_value(Title { title: text.into() }).is_ok(),
                output_accepted
            );
        }
        cases.push(json!({"input":input_value,"accepted":accepted,"normalized":normalized,"outputAccepted":output_accepted}));
    }
    if let Ok(path) = std::env::var("FORKLAUNCH_NORMALIZATION_FIXTURE") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"cases":cases,"inputSchema":input,"outputSchema":output}),
            )
            .unwrap(),
        )
        .unwrap();
    }
}

#[cfg(feature = "build")]
#[test]
fn directional_assets_reach_both_clients() {
    use forklaunch_core::{
        codegen::{SdkGenerationPlan, SdkTarget},
        contract::{ContractRegistry, ServiceContract},
    };
    let endpoint = Endpoint::<JsonBody<Title>, JsonBody<Title>>::new(
        "normalize",
        "Normalize",
        HttpMethod::Post,
        "/normalize",
        200,
    )
    .unwrap();
    let mut registry = ContractRegistry::for_service(ServiceContract::new("normalization", "1"));
    registry.register(endpoint.contract());
    let openapi: serde_json::Value =
        serde_json::from_str(&forklaunch_core::openapi::generate_openapi_json(&registry)).unwrap();
    assert_eq!(
        openapi["components"]["schemas"]["TitleWireInput"]["properties"]["title"]["maxLength"],
        12
    );
    assert_eq!(
        openapi["components"]["schemas"]["Title"]["properties"]["title"]["maxLength"],
        4
    );
    if let Ok(path) = std::env::var("FORKLAUNCH_NORMALIZATION_SDK_DIR") {
        let path = std::path::Path::new(&path);
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(
            path.join("client.ts"),
            forklaunch_core::typescript::generate_typescript_http_sdk_source(
                &SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry),
                &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("normalization"),
            )
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            path.join("client.rs"),
            forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
                &SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry),
                &forklaunch_core::rust_sdk::RustSdkOptions::new("normalization"),
            )
            .unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn metadata_request_and_response_headers_keep_their_direction() {
    use forklaunch_core::contract::endpoint::Fields;
    let endpoint = Endpoint::<Fields<JsonBody<Title>, Title, Title>, JsonBody<Title>>::new(
        "normalize",
        "Normalize",
        HttpMethod::Post,
        "/normalize",
        200,
    )
    .unwrap()
    .with_typed_response_headers::<Title>()
    .unwrap();
    let route = endpoint.contract();
    for field in route.request.query.iter().chain(&route.request.headers) {
        let schema: serde_json::Value =
            serde_json::from_str(field.schema.json_schema.as_ref().unwrap()).unwrap();
        assert_eq!(schema["maxLength"], 12);
    }
    let schema: serde_json::Value = serde_json::from_str(
        route.responses[0].headers[0]
            .schema
            .json_schema
            .as_ref()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(schema["maxLength"], 4);
}

#[test]
fn directional_projection_preserves_literals_and_rejects_unknown_codecs() {
    let literal =
        json!({"type":"string","examples":[{"x-forklaunch-string-codec":{"version":99}}]});
    assert_eq!(
        forklaunch_core::contract::codec::input_schema(&literal),
        literal
    );
    assert_eq!(
        forklaunch_core::contract::codec::input_schema(
            &json!({"type":"string","x-forklaunch-string-codec":{"version":99}})
        ),
        json!(false)
    );
}
