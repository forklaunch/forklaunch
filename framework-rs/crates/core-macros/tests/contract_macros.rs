use forklaunch_core::contract::ContractSchema;
use forklaunch_core::contract::{assert_route_params, route_operation_name};

#[forklaunch_core::contract::schema]
struct CreateUserRequest;

#[test]
fn derives_contract_schema_ref() {
    let schema = CreateUserRequest::schema_ref();

    assert_eq!(schema.name, "CreateUserRequest");
    assert!(schema.json_schema.is_some());
}

#[test]
fn normalizes_operation_names_at_compile_time() {
    assert_eq!(route_operation_name!("Create User"), "createUser");
}

#[test]
fn validates_route_params_at_compile_time() {
    assert_route_params!("/users/:id/posts/{postId}" => ["id", "postId"]);
}

#[forklaunch_core::contract::schema]
#[derive(Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct CreateRecord {
    display_name: String,
    labels: Vec<String>,
    description: Option<String>,
    state: RecordState,
}

#[forklaunch_core::contract::schema]
#[derive(Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
enum RecordState {
    Pending,
    Ready,
}

#[forklaunch_core::contract::schema]
struct Page<T> {
    items: Vec<T>,
}

#[test]
fn model_defines_both_json_and_contract_without_serde_dependencies_in_model_code() {
    let input = serde_json::json!({
        "displayName": "hello", "labels": ["one"], "description": null, "state": "ready"
    });
    let model: CreateRecord = serde_json::from_value(input.clone()).unwrap();
    assert_eq!(model.state, RecordState::Ready);
    assert_eq!(serde_json::to_value(model).unwrap(), input);

    let contract = forklaunch_core::contract::BodyContract::json_type::<CreateRecord>();
    assert_eq!(contract.kind, forklaunch_core::contract::BodyKind::Json);
    assert_eq!(contract.content_type.as_deref(), Some("application/json"));
    let schema: serde_json::Value =
        serde_json::from_str(contract.schema.json_schema.as_deref().unwrap()).unwrap();
    assert_eq!(schema["properties"]["displayName"]["type"], "string");
    assert!(
        schema["required"]
            .as_array()
            .unwrap()
            .contains(&"displayName".into())
    );
    assert!(
        !schema["required"]
            .as_array()
            .unwrap()
            .contains(&"description".into())
    );
    assert_eq!(
        schema["definitions"]["RecordState"]["enum"],
        serde_json::json!(["pending", "ready"])
    );
    assert!(
        serde_json::from_value::<CreateRecord>(serde_json::json!({"displayName": 42})).is_err()
    );
}

#[test]
fn generic_models_keep_their_nested_schema() {
    let schema = Page::<CreateRecord>::schema_ref();
    let value: serde_json::Value = serde_json::from_str(&schema.json_schema.unwrap()).unwrap();
    assert_eq!(value["properties"]["items"]["type"], "array");
    assert!(value["definitions"]["CreateRecord"].is_object());
}

#[test]
fn representation_selects_media_type_and_contract_kind_together() {
    use forklaunch_core::contract::{BodyContract, BodyKind, ContentType};
    let html = BodyContract::typed::<String>(ContentType::Html);
    assert_eq!(html.kind, BodyKind::Text);
    assert_eq!(
        html.content_type.as_deref(),
        Some("text/html; charset=utf-8")
    );
    let form = BodyContract::typed::<CreateRecord>(ContentType::Form);
    assert_eq!(form.kind, BodyKind::UrlEncoded);
    assert_eq!(
        form.content_type.as_deref(),
        Some("application/x-www-form-urlencoded")
    );
}

#[forklaunch_core::contract::schema(rename_all = "camelCase", unknown_fields = "deny")]
#[derive(Debug)]
struct ConstrainedTitle {
    #[field(min_length = 1, max_length = 2)]
    display_name: String,
}

#[test]
fn authoritative_string_constraints_match_schema_and_both_wire_directions() {
    let schema: serde_json::Value =
        serde_json::from_str(&ConstrainedTitle::schema_ref().json_schema.unwrap()).unwrap();
    let validate = forklaunch_core::contract::schema_validation::compile_checked(&schema).unwrap();
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["properties"]["displayName"]["minLength"], 1);
    assert_eq!(schema["properties"]["displayName"]["maxLength"], 2);
    // Unicode scalar count, not UTF-8 bytes or UTF-16 code units.
    for title in ["a", "🚀", "🚀é", "e\u{301}"] {
        let value = serde_json::json!({"displayName": title});
        assert!(validate(&value).is_ok());
        let model: ConstrainedTitle = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(model).unwrap(), value);
    }
    for title in ["", "abc", "🚀🚀🚀"] {
        assert!(validate(&serde_json::json!({"displayName":title})).is_err());
        assert!(
            serde_json::from_value::<ConstrainedTitle>(serde_json::json!({"displayName":title}))
                .is_err()
        );
        // Public struct construction cannot smuggle invalid output past serialization.
        assert!(
            serde_json::to_value(ConstrainedTitle {
                display_name: title.into()
            })
            .is_err()
        );
    }
    assert!(
        serde_json::from_value::<ConstrainedTitle>(
            serde_json::json!({"displayName":"a","unknown":true})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<ConstrainedTitle>(serde_json::json!({"displayName":null}))
            .is_err()
    );
    assert!(serde_json::from_value::<ConstrainedTitle>(serde_json::json!({})).is_err());
}

#[forklaunch_core::contract::schema]
#[serde(rename_all = "camelCase")]
struct LegacyHealth {
    #[field(min = 0, max = 9007199254740991)]
    record_count: i64,
}
#[test]
fn simple_legacy_container_rename_preserves_checked_field_constraints() {
    let valid: LegacyHealth = serde_json::from_value(serde_json::json!({"recordCount":1})).unwrap();
    assert_eq!(valid.record_count, 1);
    assert!(serde_json::from_value::<LegacyHealth>(serde_json::json!({"recordCount":-1})).is_err());
    assert!(serde_json::to_value(LegacyHealth { record_count: -1 }).is_err());
    let schema: serde_json::Value =
        serde_json::from_str(&LegacyHealth::schema_ref().json_schema.unwrap()).unwrap();
    assert_eq!(schema["properties"]["recordCount"]["minimum"], 0.0);
}
