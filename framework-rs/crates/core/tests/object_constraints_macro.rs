#![cfg(all(feature = "typed-schema", feature = "schema-validation"))]
use forklaunch_core::contract::{
    HttpMethod,
    endpoint::{Endpoint, Fields, NoBody, TextBody},
    schema::Optional,
};
use serde_json::json;
#[forklaunch_core::contract::schema(
    rename_all = "camelCase",
    min_properties = 1,
    max_properties = 2
)]
struct Query {
    #[field(optional, required_with = "cursor")]
    page_size: Optional<String>,
    #[field(optional, rename = "after")]
    cursor: Optional<String>,
    #[field(optional)]
    extra: Optional<String>,
}
#[test]
fn native_object_options_reach_typed_fields_and_checked_group_validation() {
    let endpoint = Endpoint::<Fields<NoBody, Query>, TextBody>::new(
        "query",
        "query",
        HttpMethod::Get,
        "/query",
        200,
    )
    .unwrap();
    let schema: serde_json::Value = serde_json::from_str(
        &endpoint
            .contract()
            .request
            .query_schema
            .unwrap()
            .json_schema
            .unwrap(),
    )
    .unwrap();
    assert_eq!(schema["dependencies"]["after"], json!(["pageSize"]));
    assert_eq!(schema["minProperties"], 1);
    assert_eq!(schema["maxProperties"], 2);
    let validate = forklaunch_core::contract::schema_validation::compile_checked(&schema).unwrap();
    for (value, valid) in [
        (json!({}), false),
        (json!({"pageSize":"10"}), true),
        (json!({"after":"cursor"}), false),
        (json!({"pageSize":"10","after":"cursor"}), true),
        (json!({"pageSize":"10","after":"cursor","extra":"x"}), false),
    ] {
        assert_eq!(validate(&value).is_ok(), valid);
    }
    assert!(schema.get("x-forklaunch-model").is_some());
}
