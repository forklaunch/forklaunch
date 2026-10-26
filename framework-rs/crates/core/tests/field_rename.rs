#![cfg(all(feature = "typed-schema", feature = "schema-validation"))]
use forklaunch_core::contract::{schema, schema::Patch, schema_validation::compile_checked};
use serde_json::json;

#[schema(rename_all = "camelCase", unknown_fields = "deny")]
struct Headers {
    #[field(rename = "last-event-id", min_length = 1, max_length = 4)]
    last_event_id: String,
    #[field(rename = "x-retry")]
    retry_count: Option<u8>,
    #[field(rename = "x-label", patch)]
    label: Patch<String>,
}

#[test]
fn wire_names_bind_validation_and_serialization() {
    let schema = forklaunch_core::contract::schema_value_for::<Headers>();
    let validate = compile_checked(&schema).unwrap();
    for (input, valid) in [
        (json!({"last-event-id":"one"}), true),
        (
            json!({"last-event-id":"one","x-retry":255,"x-label":null}),
            true,
        ),
        (json!({"last-event-id":"one","x-retry":256}), false),
        (json!({"last-event-id":""}), false),
        (json!({"lastEventId":"one"}), false),
    ] {
        assert_eq!(validate(&input).is_ok(), valid);
        assert_eq!(serde_json::from_value::<Headers>(input).is_ok(), valid);
    }
    let value = Headers {
        last_event_id: "one".into(),
        retry_count: Some(1),
        label: Patch::Keep,
    };
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        json!({"last-event-id":"one","x-retry":1})
    );
    let invalid = Headers {
        last_event_id: String::new(),
        retry_count: None,
        label: Patch::Clear,
    };
    assert!(serde_json::to_value(invalid).is_err());
}
