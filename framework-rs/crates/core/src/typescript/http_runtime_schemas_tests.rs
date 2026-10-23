use super::*;
use crate::codegen::{SdkField, SdkTarget};
use crate::contract::{
    BodyContract, ContractRegistry, RouteContract, schema_validation::compile_checked,
};
use crate::typescript::generate_typescript_operation_descriptors;
use serde_json::{Value, json};

fn mixed() -> Value {
    json!({"$id":"https://schemas.fixture.test/Mutual/index.json", "$ref":"nodes/a", "$defs":{
        "A":{"$schema":"http://json-schema.org/draft-04/schema#", "id":"nodes/a",
            "type":"object", "additionalProperties":false, "required":["count","peer"],
            "properties":{"count":{"type":"integer","minimum":0,"exclusiveMinimum":true,"const":99},
                "peer":{"$ref":"b"}}},
        "B":{"$id":"nodes/b", "type":"object", "additionalProperties":false,
            "required":["children"], "properties":{"children":{"type":"array","items":{"$ref":"a"}}}}
    }})
}

fn plan(schema: &Value) -> SdkGenerationPlan {
    let mut registry = ContractRegistry::new();
    let body = BodyContract {
        schema: SchemaRef::named("Mutual").with_json_schema(schema.to_string()),
        ..BodyContract::json("Mutual")
    };
    registry.register(
        RouteContract::post("Read Mutual", "Mixed resources", "/mutual")
            .with_request_body(body.clone())
            .with_response(200, Some(body)),
    );
    SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry)
}

fn assert_mixed(schema: &Value) {
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    let validate = compile_checked(schema).unwrap();
    assert!(
        validate(&json!({"count":1,"peer":{"children":[{"count":2,"peer":{"children":[]}}]}}))
            .is_ok()
    );
    for invalid in [
        json!({"count":0,"peer":{"children":[]}}),
        json!({"count":1.5,"peer":{"children":[]}}),
        json!({"count":1,"peer":{"children":[{"count":"wrong","peer":{"children":[]}}]}}),
        json!({"count":1,"peer":{"children":[]},"extra":true}),
    ] {
        assert!(validate(&invalid).is_err(), "{invalid}");
    }
}

#[test]
fn typescript_runtime_resources_normalize_mixed_drafts_without_changing_model_schemas() {
    let original = mixed();
    let plan = plan(&original);
    let before = plan.clone();
    let options = TypeScriptHttpSdkOptions::new("trees");
    let data = prepare_http_sdk(&plan, &options).unwrap();
    let operation = &data.services[0].routers[0].operations[0];
    let response = &operation.responses[0];
    assert_eq!(
        serde_json::from_str::<Value>(&response.body_schema_json).unwrap(),
        original
    );
    let expected = crate::codegen::runtime_schemas::schema(&original).unwrap();
    assert_mixed(&expected);
    assert_eq!(response.body_schema, escape(&expected.to_string()));
    assert_eq!(operation.request_schema, escape(&expected.to_string()));
    let descriptors = generate_typescript_operation_descriptors(&plan, &options).unwrap();
    let descriptor = &descriptors.values().next().unwrap();
    assert_eq!(descriptor["requestSchema"], expected);
    assert_eq!(
        descriptor["responseSchemas"]["200:application/json"],
        expected
    );
    assert_eq!(plan, before);
    let source = generate_typescript_http_sdk_source(&plan, &options).unwrap();
    let files = generate_typescript_http_sdk_files(&plan, &options).unwrap();
    assert!(source.contains(&format!("JSON.parse('{}')", escape(&expected.to_string()))));
    assert!(
        files["client.ts"].contains(&format!("JSON.parse('{}')", escape(&expected.to_string())))
    );
    // The actual public ESM/CJS runtime probe consumes this compiler output.
    if let Ok(directory) = std::env::var("FORKLAUNCH_TEST_TS_RUNTIME_SCHEMAS_OUTPUT") {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            std::path::Path::new(&directory).join("mixed-descriptor.json"),
            serde_json::to_vec_pretty(descriptor).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn typescript_runtime_resources_normalize_request_groups_and_version_slots() {
    let original = mixed();
    let mut plan = plan(&original);
    let operation = &mut plan.operations[0];
    let group = json!({"type":"object", "required":["tree"], "properties":{"tree":original}});
    operation.request_query_schema = Some(group.to_string());
    operation.request_query = vec![SdkField {
        name: "tree".into(),
        schema: "Mutual".into(),
        json_schema: Some(original.to_string()),
        required: true,
    }];
    let options = TypeScriptHttpSdkOptions::new("trees");
    let descriptors = generate_typescript_operation_descriptors(&plan, &options).unwrap();
    let groups = &descriptors.values().next().unwrap()["requestObjects"];
    assert_mixed(&groups["query"]["fields"]["tree"]["schema"]);
    let validate = compile_checked(&groups["query"]["schema"]).unwrap();
    assert!(validate(&json!({"tree":{"count":1,"peer":{"children":[]}}})).is_ok());
    assert!(validate(&json!({"tree":{"count":0,"peer":{"children":[]}}})).is_err());
    let branch = plan.operations[0].clone();
    plan.operations[0].version_branches = vec![branch];
    let descriptors = generate_typescript_operation_descriptors(&plan, &options).unwrap();
    let branch = &descriptors.values().next().unwrap()["versionMetadata"]["branches"][0];
    for pointer in [
        "/bodySchema",
        "/query/tree/schema",
        "/requestObjects/query/fields/tree/schema",
        "/responses/200:application~1json/schema",
    ] {
        assert_mixed(branch.pointer(pointer).unwrap());
    }
}

#[test]
fn typescript_runtime_resources_preserve_literals_and_fail_missing_references() {
    let literal = json!({"$id":"literal", "$ref":"#missing", "$schema":"literal"});
    let schema = json!({"type":"object", "properties":{"value":{"type":"string"}},
        "examples":[literal], "default":literal});
    let descriptors = generate_typescript_operation_descriptors(
        &plan(&schema),
        &TypeScriptHttpSdkOptions::new("trees"),
    )
    .unwrap();
    assert_eq!(
        descriptors.values().next().unwrap()["responseSchemas"]["200:application/json"],
        schema
    );
    let invalid = json!({"$id":"https://schemas.fixture.test/root", "$ref":"https://outside.invalid/missing"});
    assert!(
        generate_typescript_operation_descriptors(
            &plan(&invalid),
            &TypeScriptHttpSdkOptions::new("trees")
        )
        .is_err()
    );
}
