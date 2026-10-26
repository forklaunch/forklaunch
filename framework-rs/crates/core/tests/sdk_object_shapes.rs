#![cfg(all(feature = "typescript", feature = "typed-schema"))]

use std::collections::BTreeMap;

use forklaunch_core::{
    codegen::{SdkGenerationPlan, SdkTarget, compile_schema},
    contract::{
        ContractRegistry, HttpMethod,
        endpoint::{Endpoint, JsonBody, NoBody},
    },
    typescript::{TypeScriptHttpSdkOptions, generate_typescript_service_declarations},
};
use serde_json::json;

#[forklaunch_core::contract::schema]
pub struct Details {
    pub count: u16,
}

#[forklaunch_core::contract::schema]
pub struct ObjectPayload {
    pub title: String,
    pub details: Details,
    pub labels: BTreeMap<String, String>,
}

#[test]
fn ordinary_dtos_do_not_acquire_dictionary_keys() {
    let endpoint = Endpoint::<NoBody, JsonBody<ObjectPayload>>::new(
        "Fetch",
        "Fetch",
        HttpMethod::Get,
        "/payload",
        200,
    )
    .unwrap();
    let mut registry = ContractRegistry::new();
    registry.register(endpoint.contract());
    let source = generate_typescript_service_declarations(
        &SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry),
        &TypeScriptHttpSdkOptions::new("objects"),
    )
    .unwrap();
    assert!(!source.contains("& Record<string, unknown>"));
    assert!(source.contains("labels: Record<string, string>"));
    if let Some(directory) = std::env::var_os("FORKLAUNCH_OBJECT_SDK_FIXTURE") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("sdk.ts"), source).unwrap();
        std::fs::write(
            directory.join("consumer.ts"),
            include_str!("../compiler-tests/package_sdk/object_shapes.ts"),
        )
        .unwrap();
    }
}

#[test]
fn explicit_dictionary_and_open_object_surfaces_remain_available() {
    for (schema, expected) in [
        (
            json!({"type":"object", "properties":{"name":{"type":"string"}}, "required":["name"]}),
            "{ name: string }",
        ),
        (
            json!({"type":"object", "properties":{"name":{"type":"string"}}, "required":["name"], "additionalProperties":false}),
            "{ name: string }",
        ),
        (
            json!({"type":"object", "properties":{"name":{"type":"string"}}, "required":["name"], "additionalProperties":true}),
            "{ name: string } & Record<string, unknown>",
        ),
        (
            json!({"type":"object", "additionalProperties":{"type":"string"}}),
            "Record<string, string>",
        ),
        (json!({"type":"object"}), "Record<string, unknown>"),
        (
            json!({"type":"object", "additionalProperties":false}),
            "Record<string, never>",
        ),
    ] {
        assert_eq!(
            compile_schema("Payload", &schema).unwrap().typescript_type,
            expected
        );
    }
}
