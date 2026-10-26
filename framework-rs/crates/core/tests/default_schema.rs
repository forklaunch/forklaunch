#![cfg(all(feature = "typed-schema", feature = "schema-validation"))]
use forklaunch_core::contract::{
    HttpMethod,
    endpoint::{Endpoint, JsonBody},
    schema,
    schema_validation::compile_checked,
};
use serde_json::json;
#[schema(unknown_fields = "deny")]
struct Defaults {
    #[field(default, max_length = 4)]
    title: String,
    #[field(default, min = 0, max = 10)]
    count: i64,
    #[field(default)]
    active: bool,
    #[field(default)]
    small: u8,
}
#[test]
fn defaulted_input_members_are_required_output_values() {
    let endpoint = Endpoint::<JsonBody<Defaults>, JsonBody<Defaults>>::new(
        "apply",
        "Apply",
        HttpMethod::Post,
        "/defaults",
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
    assert_eq!(output["required"].as_array().unwrap().len(), 4);
    assert!(input["required"].as_array().unwrap().is_empty());
    let validate_input = compile_checked(&input).unwrap();
    let validate_output = compile_checked(&output).unwrap();
    let mut cases = Vec::new();
    for value in [
        json!({}),
        json!({"title":"ok"}),
        json!({"title":null}),
        json!({"count":null}),
        json!({"count":11}),
        json!({"count":-1}),
        json!({"active":true}),
        json!({"active":null}),
        json!({"small":256}),
        json!({"title":"abcde"}),
        json!({"unknown":1}),
        json!({"title":"","count":0,"active":false,"small":0}),
        json!({"title":"ok","count":10,"active":true,"small":255}),
    ] {
        let parsed = serde_json::from_value::<Defaults>(value.clone());
        let accepted = parsed.is_ok();
        assert_eq!(
            validate_input(&value).is_ok(),
            accepted,
            "raw default schema mismatch {value}"
        );
        let normalized = parsed
            .ok()
            .map(|value| serde_json::to_value(value).unwrap());
        if let Some(normalized) = &normalized {
            assert!(validate_output(normalized).is_ok());
        }
        let output_accepted = validate_output(&value).is_ok();
        cases.push(json!({"input":value,"accepted":accepted,"normalized":normalized,"outputAccepted":output_accepted}));
    }
    assert!(
        serde_json::to_value(Defaults {
            title: "abcde".into(),
            count: 0,
            active: false,
            small: 0
        })
        .is_err()
    );
    if let Ok(path) = std::env::var("FORKLAUNCH_DEFAULT_FIXTURE") {
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
fn default_assets_reach_both_clients() {
    use forklaunch_core::{
        codegen::{SdkGenerationPlan, SdkTarget},
        contract::{ContractRegistry, ServiceContract},
    };
    let endpoint = Endpoint::<JsonBody<Defaults>, JsonBody<Defaults>>::new(
        "apply",
        "Apply",
        HttpMethod::Post,
        "/defaults",
        200,
    )
    .unwrap();
    let mut registry = ContractRegistry::for_service(ServiceContract::new("defaults", "1"));
    registry.register(endpoint.contract());
    let openapi: serde_json::Value =
        serde_json::from_str(&forklaunch_core::openapi::generate_openapi_json(&registry)).unwrap();
    assert!(
        openapi["components"]["schemas"]["DefaultsWireInput"]["required"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        openapi["components"]["schemas"]["Defaults"]["required"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    if let Ok(path) = std::env::var("FORKLAUNCH_DEFAULT_SDK_DIR") {
        let path = std::path::Path::new(&path);
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(
            path.join("client.ts"),
            forklaunch_core::typescript::generate_typescript_http_sdk_source(
                &SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry),
                &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("defaults"),
            )
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            path.join("client.rs"),
            forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
                &SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry),
                &forklaunch_core::rust_sdk::RustSdkOptions::new("defaults"),
            )
            .unwrap(),
        )
        .unwrap();
    }
}
