//! Compiled JSON Schema validation for generated clients and raw socket contracts.
use crate::ws::codec::{JsonValidator, PayloadError};
use serde_json::Value;
use std::sync::Arc;

pub(crate) fn compile(schema: &Value) -> Result<Arc<JsonValidator>, PayloadError> {
    use crate::contract::schema_validation::{SchemaValidationFailure, compile_checked};
    let validate = compile_checked(schema)
        .map_err(|error| PayloadError::Invalid(error.replace("JSON schema", "socket schema")))?;
    Ok(Arc::new(move |value| {
        validate(value).map_err(|error| match error {
            SchemaValidationFailure::Invalid(message) => PayloadError::Invalid(message),
            SchemaValidationFailure::Operational => PayloadError::Operational,
        })
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ws::codec::PayloadCodec;
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    #[test]
    fn ajv_reference_tokens_and_sibling_constraints_preserve_schema_literals() {
        for dialect in [
            "http://json-schema.org/draft-04/schema#",
            "http://json-schema.org/draft-06/schema#",
            "http://json-schema.org/draft-07/schema#",
            "https://json-schema.org/draft/2019-09/schema",
            "https://json-schema.org/draft/2020-12/schema",
        ] {
            let schema = json!({
                "$schema": dialect,
                "$ref": "#/definitions/Foo%2FBar",
                "definitions": {
                    "Foo/Bar": {"type": "string"},
                    "Foo": {"Bar": {"type": "number"}}
                },
                "minLength": 3,
                "maxLength": 8,
                "default": {"$ref": "literal %2F data"}
            });
            let original = schema.clone();
            let codec = PayloadCodec::<Value>::json_schema(&schema).unwrap();
            assert!(codec.encode(&json!("hello")).is_ok(), "{dialect}");
            for invalid in [json!("hi"), json!("too many characters"), json!(42)] {
                assert!(codec.encode(&invalid).is_err(), "{dialect}: {invalid}");
            }
            assert_eq!(schema, original);
        }
        let schema = json!({
            "$ref": "#/default/ref%2Fpath",
            "default": {"ref/path": {"type": "object"}},
            "const": {"$ref": "literal"}
        });
        let codec = PayloadCodec::<Value>::json_schema(&schema).unwrap();
        assert!(codec.encode(&json!({"$ref": "literal"})).is_ok());
        assert!(codec.encode(&json!({})).is_err());
    }
    #[test]
    fn schema_validation_precedes_deserialization_and_preserves_original_outgoing_strings() {
        #[derive(Serialize, Deserialize)]
        struct Input {
            message: String,
        }
        let codec = PayloadCodec::<Input>::json_schema(&json!({"type":"object","required":["message"],"properties":{"message":{"type":"string","minLength":1}},"additionalProperties":false})).unwrap();
        assert!(codec.decode(br#"{"message":"hello"}"#).is_ok());
        assert!(
            codec
                .decode(br#"{"message":"hello","extra":true}"#)
                .is_err()
        );
        assert!(codec.decode(br#"{"message":""}"#).is_err());
        assert!(
            codec
                .encode(&Input {
                    message: String::new()
                })
                .is_err()
        );
        let string = PayloadCodec::<String>::json_schema(&json!({"type":"string"})).unwrap();
        assert_eq!(string.encode(&"42".into()).unwrap(), b"42");
        assert!(string.decode(b"42").is_err()); // The canonical raw-string wire ambiguity is retained.
        assert_eq!(string.decode(b"hello").unwrap(), "hello");
        let bounded = string.with_max_bytes(4).unwrap();
        assert!(matches!(
            bounded.encode(&"large".into()),
            Err(PayloadError::Limit { .. })
        ));
    }
    #[test]
    fn integer_spellings_follow_javascript_json_numbers_before_typed_deserialization() {
        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        struct Integers {
            value: i64,
            nested: Vec<i64>,
        }
        for dialect in [
            "http://json-schema.org/draft-04/schema#",
            "http://json-schema.org/draft-07/schema#",
        ] {
            let schema = json!({"$schema":dialect,"type":"object","required":["value","nested"],"properties":{"value":{"type":"integer"},"nested":{"type":"array","items":{"type":"integer"}}}});
            let codec = PayloadCodec::<Integers>::json_schema(&schema).unwrap();
            assert_eq!(
                codec
                    .decode(br#"{"value":1.0,"nested":[1e0,-0.0]}"#)
                    .unwrap(),
                Integers {
                    value: 1,
                    nested: vec![1, 0]
                }
            );
            assert!(codec.decode(br#"{"value":1.5,"nested":[]}"#).is_err());
            assert!(codec.decode(br#"{"value":"1","nested":[]}"#).is_err());
        }
        let codec = PayloadCodec::<f64>::json_schema(
            &json!({"$schema":"http://json-schema.org/draft-04/schema#","type":"integer"}),
        )
        .unwrap();
        assert_eq!(codec.encode(&1.0).unwrap(), b"1.0");
        assert!(codec.encode(&1.5).is_err());
        let value = PayloadCodec::<Value>::json_schema(
            &json!({"$schema":"http://json-schema.org/draft-04/schema#","type":"integer"}),
        )
        .unwrap();
        assert!(
            value
                .decode(b"-0.0")
                .unwrap()
                .as_f64()
                .unwrap()
                .is_sign_negative()
        );
        // Generic serde codecs retain their explicitly stricter integer decoding.
        assert!(PayloadCodec::<i64>::serde().decode(b"1.0").is_err());
    }

    #[test]
    fn invalid_schemas_and_external_retrieval_fail_without_reading_resources() {
        for schema in [
            json!({"type":"invalid"}),
            json!({"$ref":"#/missing"}),
            json!({"$ref":"https://example.com/unavailable"}),
            json!({"$ref":"file:///unavailable"}),
        ] {
            assert!(PayloadCodec::<Value>::json_schema(&schema).is_err());
        }
        let file =
            std::env::temp_dir().join(format!("forklaunch-ws-schema-{}.json", std::process::id()));
        std::fs::write(&file, r#"{"type":"string"}"#).unwrap();
        let result = PayloadCodec::<Value>::json_schema(
            &json!({"$ref":format!("file://{}",file.display())}),
        );
        std::fs::remove_file(file).unwrap();
        let Err(error) = result else {
            panic!(
                "external file retrieval must remain disabled even when dependency features enable it"
            );
        };
        assert!(
            error
                .to_string()
                .contains("socket schema resource retrieval is disabled"),
            "{error}"
        );
    }
    #[test]
    fn schema_compilation_bounds_depth_nodes_bytes_and_regex_size() {
        let mut schema = json!(true);
        for _ in 0..66 {
            schema = json!({"properties":{"next":schema}});
        }
        assert!(PayloadCodec::<Value>::json_schema(&schema).is_err());
        assert!(PayloadCodec::<Value>::json_schema(&json!({"enum":vec![0;100_001]})).is_err());
        assert!(
            PayloadCodec::<Value>::json_schema(&json!({"title":"x".repeat(32 * 1024 * 1024)}))
                .is_err()
        );
        assert!(PayloadCodec::<Value>::json_schema(&json!({"pattern":"["})).is_err());
    }
    #[test]
    fn json_schema_payload_conformance() {
        let Ok(input) = std::env::var("FORKLAUNCH_TEST_WS_JSON_SCHEMA_INPUT") else {
            return;
        };
        let cases: Vec<Value> = serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
        let results: Vec<_> = cases
            .iter()
            .map(|case| {
                let Ok(codec) = PayloadCodec::<Value>::json_schema(&case["schema"]) else {
                    return json!({"ok":false,"compileError":true});
                };
                if case["action"] == "encode" {
                    match codec.encode(&case["value"]) {
                        Ok(bytes) => json!({"ok":true,"bytes":bytes}),
                        Err(_) => json!({"ok":false}),
                    }
                } else {
                    let bytes: Vec<u8> = serde_json::from_value(case["bytes"].clone()).unwrap();
                    match codec.decode(&bytes) {
                        Ok(value) => json!({"ok":true,"value":value}),
                        Err(_) => json!({"ok":false}),
                    }
                }
            })
            .collect();
        std::fs::write(
            std::env::var("FORKLAUNCH_TEST_WS_JSON_SCHEMA_OUTPUT").unwrap(),
            serde_json::to_vec(&results).unwrap(),
        )
        .unwrap();
    }
}

#[cfg(all(test, feature = "zod-regex"))]
mod regex_tests {
    use super::*;
    #[test]
    fn codec_preserves_operational_failure_in_both_directions() {
        let schema = serde_json::json!({"type":"string","x-forklaunch-zod-regex":{"version":1,"rules":[{"sourceUnits":"^(a+)+$".encode_utf16().collect::<Vec<_>>(),"flags":""}]}});
        let codec = crate::ws::codec::PayloadCodec::<String>::json_schema(&schema).unwrap();
        let input = format!("{}!", "a".repeat(40));
        assert_eq!(
            codec.decode(input.as_bytes()),
            Err(PayloadError::Operational)
        );
        assert_eq!(codec.encode(&input), Err(PayloadError::Operational));
        assert!(codec.encode(&"a".into()).is_ok());
    }
}
