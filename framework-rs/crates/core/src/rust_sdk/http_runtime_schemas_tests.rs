use super::*;
use crate::contract::schema_validation::compile_checked;
use serde_json::json;

fn mutual() -> Value {
    json!({"$ref":"#A","$defs":{
        "a":{"$anchor":"A","type":"object","required":["name","children"],"additionalProperties":false,
            "properties":{"name":{"type":"string"},"children":{"type":"array","items":{"$ref":"#B"}}}},
        "b":{"$anchor":"B","type":"object","required":["parent"],"additionalProperties":false,
            "properties":{"parent":{"anyOf":[{"$ref":"#A"},{"type":"null"}]}}}
    }})
}

#[test]
fn runtime_resources_resolve_mutual_anchors_without_rewriting_literals() {
    let literal = json!({"$anchor":"A","$ref":"#missing","$schema":"literal"});
    let mut original = mutual();
    original["examples"] = json!([literal]);
    original["default"] = literal.clone();
    let normalized = schema(&original).unwrap();
    assert_eq!(normalized["examples"], original["examples"]);
    assert_eq!(normalized["default"], literal);
    assert!(normalized.get("$schema").is_none());
    let validate = compile_checked(&normalized).unwrap();
    assert!(validate(&json!({"name":"root","children":[{"parent":null}]})).is_ok());
    assert!(
        validate(&json!({"name":"root","children":[{"parent":{"name":"leaf","children":[]}}]}))
            .is_ok()
    );
    assert!(
        validate(&json!({"name":"root","children":[{"parent":{"name":2,"children":[]}}]})).is_err()
    );
    assert!(validate(&json!({"name":"root","children":[{"parent":null,"extra":true}]})).is_err());
    assert_eq!(original["$defs"]["a"]["$anchor"], "A");

    let enumerated =
        json!({"$id":"https://fixture.invalid/enum","enum":[literal],"default":literal});
    let normalized = schema(&enumerated).unwrap();
    assert_eq!(normalized["enum"], enumerated["enum"]);
    assert_eq!(normalized["default"], enumerated["default"]);
    assert!(compile_checked(&normalized).unwrap()(&literal).is_ok());
}

#[test]
fn runtime_resources_preserve_explicit_drafts_and_lexical_resource_ids() {
    let original = json!({
        "$schema":"http://json-schema.org/draft-04/schema#","id":"https://fixture.invalid/root",
        "$ref":"nodes#record","$defs":{"node":{
            "id":"nodes#record","type":"object","required":["count"],"additionalProperties":false,
            "properties":{"count":{"type":"integer","minimum":0,"exclusiveMinimum":true,"const":99}}
        }}
    });
    let normalized = schema(&original).unwrap();
    assert_eq!(normalized["$schema"], original["$schema"]);
    let validate = compile_checked(&normalized).unwrap();
    assert!(validate(&json!({"count":1})).is_ok()); // Draft 4 ignores const.
    assert!(validate(&json!({"count":0})).is_err()); // Boolean exclusive bound remains active.
    assert!(validate(&json!({"count":1.5})).is_err());
    assert!(validate(&json!({"count":1,"extra":true})).is_err());
    let modern = json!({"$schema":"https://json-schema.org/draft/2020-12/schema",
    "$id":"https://fixture.invalid/root", "$ref":"child#value", "$defs":{
        "child":{"$id":"child","$anchor":"value","type":"string","const":"exact"}
    }});
    let normalized = schema(&modern).unwrap();
    assert_eq!(normalized["$schema"], modern["$schema"]);
    let validate = compile_checked(&normalized).unwrap();
    assert!(validate(&json!("exact")).is_ok());
    assert!(validate(&json!("other")).is_err());
}

#[test]
fn runtime_resources_visit_all_request_and_response_metadata_slots() {
    let original = mutual();
    let field = json!({"required":true,"schema":original});
    let groups = json!({"query":{"schema":original,"fields":{"tree":field}}});
    let metadata = json!({"formatVersion":1,"branches":[{
        "bodySchema":original,"params":{"tree":field},"query":{"tree":field},"headers":{"tree":field},
        "requestObjects":groups,"responses":{"200:application/json":{"schema":original,"headers":{"tree":field}}}
    }]});
    let compiled: Value = serde_json::from_str(&versions(&metadata.to_string()).unwrap()).unwrap();
    for path in [
        "/bodySchema",
        "/params/tree/schema",
        "/query/tree/schema",
        "/headers/tree/schema",
        "/requestObjects/query/schema",
        "/requestObjects/query/fields/tree/schema",
        "/responses/200:application~1json/schema",
        "/responses/200:application~1json/headers/tree/schema",
    ] {
        let emitted = compiled["branches"][0].pointer(path).unwrap();
        let validate = compile_checked(emitted).unwrap();
        assert!(
            validate(&json!({"name":"valid","children":[]})).is_ok(),
            "{path}"
        );
        assert!(
            validate(&json!({"name":false,"children":[]})).is_err(),
            "{path}"
        );
    }
    let groups: Value =
        serde_json::from_str(&request_objects(&groups.to_string()).unwrap()).unwrap();
    assert!(compile_checked(&groups["query"]["schema"]).is_ok());
    let fields: Value =
        serde_json::from_str(&fields(&json!({"tree":field}).to_string()).unwrap()).unwrap();
    assert!(compile_checked(&fields["tree"]["schema"]).is_ok());
}

#[test]
fn runtime_resources_are_emitted_by_both_native_rust_sdk_layouts() {
    use crate::codegen::{SdkGenerationPlan, SdkTarget};
    use crate::contract::{BodyContract, ContractRegistry, RouteContract, SchemaRef};
    use crate::rust_sdk::{
        RustSdkOptions, generate_rust_http_sdk_files, generate_rust_http_sdk_source,
    };
    let mut registry = ContractRegistry::new();
    registry.register(
        RouteContract::get("Read", "Recursive resource", "/tree").with_response(
            200,
            Some(BodyContract {
                schema: SchemaRef::named("Tree").with_json_schema(mutual().to_string()),
                ..BodyContract::json("Tree")
            }),
        ),
    );
    let plan = SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry);
    let options = RustSdkOptions::new("trees");
    let data = super::super::prepare_http_sdk(&plan, &options, true).unwrap();
    let emitted: String =
        serde_json::from_str(&data.operations[0].responses[0].rust_body_schema).unwrap();
    assert!(compile_checked(&serde_json::from_str(&emitted).unwrap()).is_ok());
    assert_eq!(
        serde_json::from_str::<Value>(&data.operations[0].responses[0].body_schema_json).unwrap(),
        mutual()
    );
    let files = generate_rust_http_sdk_files(&plan, &options).unwrap();
    assert!(files["schemas.rs"].contains("READ_STATUS200_BODY"));
    assert!(!files["schemas.rs"].contains("$anchor"));
    assert!(
        generate_rust_http_sdk_source(&plan, &options)
            .unwrap()
            .contains(&format!("{emitted:?}"))
    );
}

#[test]
fn runtime_resources_reject_external_missing_and_conflicting_scopes() {
    for original in [
        json!({"$id":"https://fixture.invalid/root","$ref":"https://outside.invalid/missing"}),
        json!({"$ref":"#missing","$defs":{"a":{"$anchor":"exists","type":"string"}}}),
        json!({"$defs":{"a":{"$anchor":"same","type":"string"},"b":{"$anchor":"same","type":"number"}}}),
        json!({"$schema":"http://json-schema.org/draft-04/schema#","$anchor":"modern"}),
    ] {
        assert!(schema(&original).is_err(), "{original}");
    }
}

#[test]
fn runtime_resources_compile_anchored_whole_request_groups() {
    use crate::codegen::{SdkGenerationPlan, SdkTarget};
    use crate::contract::{ContractRegistry, FieldContract, RouteContract, SchemaRef};
    use crate::rust_sdk::{RustSdkOptions, generate_rust_http_sdk_files};
    let original = json!({"type":"object","properties":{"label":{"$ref":"#label"}},
        "additionalProperties":false,"$defs":{"label":{"$anchor":"label","type":"string","minLength":2}},
        "default":{"$ref":"#literal"}});
    let mut route = RouteContract::get("Search", "Anchored query", "/search")
        .with_query(FieldContract::optional("label", "String"))
        .with_response(204, None);
    route.request.query[0].schema =
        SchemaRef::named("String").with_json_schema(json!({"type":"string"}).to_string());
    route.request.query_schema =
        Some(SchemaRef::named("Query").with_json_schema(original.to_string()));
    let mut registry = ContractRegistry::new();
    registry.register(route);
    let plan = SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry);
    let options = RustSdkOptions::new("search");
    let data = super::super::prepare_http_sdk(&plan, &options, true).unwrap();
    let encoded: String = serde_json::from_str(&data.operations[0].request_objects).unwrap();
    let emitted: Value = serde_json::from_str(&encoded).unwrap();
    let validate = compile_checked(&emitted["query"]["schema"]).unwrap();
    assert!(validate(&json!({"label":"valid"})).is_ok());
    assert!(validate(&json!({"label":"x"})).is_err());
    assert!(validate(&json!({"extra":"no"})).is_err());
    assert_eq!(emitted["query"]["schema"]["default"], original["default"]);
    assert!(
        generate_rust_http_sdk_files(&plan, &options).unwrap()["schemas.rs"]
            .contains("SEARCH_REQUEST_FIELDS")
    );
    let portable: Value = serde_json::from_str(
        &crate::codegen::request_objects::metadata(&plan.operations[0]).unwrap(),
    )
    .unwrap();
    assert_eq!(portable["query"]["schema"], original);
}
