#![cfg(all(feature = "typed-schema", feature = "schema-validation"))]
use forklaunch_core::contract::{schema, schema::Optional, schema_validation::compile_checked};
use serde_json::json;
#[schema(unknown_fields = "deny")]
struct Update {
    #[field(optional)]
    title: Optional<String>,
    #[field(optional)]
    nullable: Optional<Option<u8>>,
}
#[test]
fn absence_does_not_imply_nullability() {
    let schema = forklaunch_core::contract::schema_value_for::<Update>();
    let validate = compile_checked(&schema).unwrap();
    let mut cases = Vec::new();
    for (input, valid) in [
        (json!({}), true),
        (json!({"title":""}), true),
        (json!({"title":null}), false),
        (json!({"title":false}), false),
        (json!({"nullable":null}), true),
        (json!({"nullable":255}), true),
        (json!({"nullable":256}), false),
        (json!({"nullable":-1}), false),
        (json!({"nullable":1.5}), false),
        (json!({"extra":1}), false),
    ] {
        assert_eq!(validate(&input).is_ok(), valid, "schema {input}");
        let parsed = serde_json::from_value::<Update>(input.clone());
        assert_eq!(parsed.is_ok(), valid, "native {input}");
        if let Ok(value) = parsed {
            assert_eq!(serde_json::to_value(value).unwrap(), input);
        }
        cases.push(json!({"input":input,"accepted":valid}));
    }
    assert!(serde_json::to_value(Optional::<String>::Missing).is_err());
    assert_eq!(Optional::<String>::Missing.as_option(), None);
    assert_eq!(
        Optional::Value(String::from("hello"))
            .as_option()
            .map(String::as_str),
        Some("hello")
    );
    assert_eq!(Optional::Value(None::<String>).as_option(), Some(&None));
    assert_eq!(Optional::Value(5).into_option(), Some(5));
    if let Ok(path) = std::env::var("FORKLAUNCH_OPTIONAL_FIXTURE") {
        std::fs::write(
            path,
            serde_json::to_vec(&json!({"schema":schema,"cases":cases})).unwrap(),
        )
        .unwrap();
    }
}

#[schema(unknown_fields = "deny")]
struct Cursor {
    #[field(rename = "last-event-id", min = 0)]
    cursor: Option<i64>,
    #[field(optional, min = 0, max = 100)]
    revision: Optional<i64>,
}
#[test]
fn numeric_bounds_apply_to_both_wire_directions() {
    let schema = forklaunch_core::contract::schema_value_for::<Cursor>();
    let validate = compile_checked(&schema).unwrap();
    for (input, valid) in [
        (json!({}), true),
        (json!({"last-event-id":null}), true),
        (json!({"last-event-id":0}), true),
        (json!({"last-event-id":-1}), false),
        (json!({"last-event-id":9_007_199_254_740_991_i64}), true),
        (json!({"last-event-id":9_007_199_254_740_992_i64}), false),
        (json!({"revision":100}), true),
        (json!({"revision":101}), false),
        (json!({"revision":null}), false),
    ] {
        assert_eq!(validate(&input).is_ok(), valid, "schema {input}");
        assert_eq!(serde_json::from_value::<Cursor>(input).is_ok(), valid);
    }
    assert!(
        serde_json::to_value(Cursor {
            cursor: Some(-1),
            revision: Optional::Missing
        })
        .is_err()
    );
    assert!(
        serde_json::to_value(Cursor {
            cursor: None,
            revision: Optional::Value(101)
        })
        .is_err()
    );
}

#[schema(unknown_fields = "deny")]
struct Rename {
    #[field(optional, rename = "display-name", min_length = 1, max_length = 4)]
    name: Optional<String>,
}
#[test]
fn optional_string_constraints_are_authoritative() {
    let schema = forklaunch_core::contract::schema_value_for::<Rename>();
    let validate = compile_checked(&schema).unwrap();
    for (input, valid) in [
        (json!({}), true),
        (json!({"display-name":null}), false),
        (json!({"display-name":""}), false),
        (json!({"display-name":"a"}), true),
        (json!({"display-name":"🍎🍎🍎🍎"}), true),
        (json!({"display-name":"abcde"}), false),
    ] {
        assert_eq!(validate(&input).is_ok(), valid, "schema {input}");
        assert_eq!(serde_json::from_value::<Rename>(input).is_ok(), valid);
    }
    assert!(
        serde_json::to_value(Rename {
            name: Optional::Value(String::new())
        })
        .is_err()
    );
    assert_eq!(
        serde_json::to_value(Rename {
            name: Optional::Missing
        })
        .unwrap(),
        json!({})
    );
}
