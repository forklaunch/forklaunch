//! Opaque file assertions compiled at their original schema/resource location.
use serde_json::{Map, Value, json};

/// Recognize standard opaque binary schemas and the exact legacy TS File marker.
#[must_use]
pub fn opaque_binary_schema(schema: &Value) -> bool {
    schema.as_object().is_some_and(opaque_binary_object)
}
fn opaque_binary_object(object: &Map<String, Value>) -> bool {
    object.get("type").and_then(Value::as_str) == Some("string")
            && object.get("format").and_then(Value::as_str) == Some("binary")
            // The TS file exporter represents Buffer/ArrayBuffer/Blob guards as
            // empty alternatives, under `oneOf` or (newer OpenAPI exporters)
            // `anyOf`. Only this exact legacy File marker is opaque.
            && ["oneOf", "anyOf"].iter().all(|keyword| {
                object.get(*keyword).is_none_or(|alternatives| {
                    object.get("title").and_then(Value::as_str) == Some("File")
                        && *alternatives == serde_json::json!([{}, {}, {}, {"type":["string"]}])
                })
            })
            && object.keys().all(|key| {
                matches!(
                    key.as_str(),
                    "type"
                        | "format"
                        | "oneOf"
                        | "anyOf"
                        | "$schema"
                        | "$id"
                        | "id"
                        | "$anchor"
                        | "$dynamicAnchor"
                        | "$recursiveAnchor"
                        | "$comment"
                        | "title"
                        | "description"
                        | "default"
                        | "examples"
                        | "example"
                        | "deprecated"
                        | "readOnly"
                        | "writeOnly"
                )
            })
}

pub(super) fn rewrite(schema: &Map<String, Value>) -> Result<Option<Value>, String> {
    if schema.get("type").and_then(Value::as_str) != Some("string")
        || schema.get("format").and_then(Value::as_str) != Some("binary")
    {
        return Ok(None);
    }
    if !opaque_binary_object(schema) {
        return Err("Multipart opaque file schemas cannot include string assertions; use a native byte-array schema for byte constraints".into());
    }
    let mut object = schema.clone();
    object.insert("type".into(), json!(["array", "string"]));
    object.remove("format");
    object.remove("oneOf");
    object.remove("anyOf");
    object.insert(
        "items".into(),
        json!({"type":"integer","minimum":0,"maximum":255}),
    );
    Ok(Some(Value::Object(object)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::schema_validation::{compile, compile_multipart};
    #[test]
    fn referenced_files_preserve_literal_assertions_and_source_schemas() {
        let schema = json!({"type":"object","required":["file","metadata"],"properties":{"file":{"$ref":"#/default/file"},"metadata":{"const":{"type":"string","format":"binary"}}},"default":{"file":{"type":"string","format":"binary","title":"Upload"}}});
        let original = schema.clone();
        let validate = compile_multipart(&schema).unwrap();
        assert_eq!(schema, original);
        for file in [json!([0, 255]), json!("text"), json!([])] {
            assert!(
                validate(&json!({"file":file,"metadata":{"type":"string","format":"binary"}}))
                    .is_ok()
            );
        }
        for file in [json!([-1]), json!([256]), json!([1.5])] {
            assert!(
                validate(&json!({"file":file,"metadata":{"type":"string","format":"binary"}}))
                    .is_err()
            );
        }
        assert!(
            validate(&json!({"file":[0],"metadata":{"type":"array","items":{"type":"integer"}}}))
                .is_err()
        );
        assert!(
            compile(&schema).unwrap()(
                &json!({"file":[0],"metadata":{"type":"string","format":"binary"}})
            )
            .is_err()
        );
    }
    #[test]
    fn scoped_files_keep_ids_anchors_and_evaluated_items() {
        let schema = json!({"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"https://example.invalid/root","type":"object","properties":{"file":{"allOf":[{"$ref":"file#bytes"}],"unevaluatedItems":false}},"$defs":{"File":{"$id":"file","$anchor":"bytes","type":"string","format":"binary"}}});
        let validate = compile_multipart(&schema).unwrap();
        assert!(validate(&json!({"file":[0,255]})).is_ok());
        assert!(validate(&json!({"file":[256]})).is_err());
        assert!(compile(&schema).unwrap()(&json!({"file":[0,255]})).is_err());
    }
    #[test]
    fn dynamic_and_recursive_file_references_preserve_scope() {
        for schema in [
            json!({"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"https://example.invalid/root","$defs":{"File":{"$id":"file","$dynamicAnchor":"bytes","type":"string","format":"binary"}},"$dynamicRef":"file#bytes"}),
            json!({"$schema":"https://json-schema.org/draft/2019-09/schema","$id":"https://example.invalid/root","$defs":{"File":{"$id":"file","$recursiveAnchor":true,"type":"string","format":"binary"}},"$ref":"file"}),
            json!({"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"https://example.invalid/root","$defs":{"File":{"$schema":"http://json-schema.org/draft-07/schema#","$id":"file","type":"string","format":"binary"}},"$ref":"file"}),
        ] {
            let validate = compile_multipart(&schema).unwrap();
            for value in [json!([]), json!([0, 255]), json!("text")] {
                assert!(validate(&value).is_ok());
            }
            for value in [json!([256]), json!([-1]), json!([0.5]), json!({})] {
                assert!(validate(&value).is_err());
            }
            assert!(compile(&schema).unwrap()(&json!([0])).is_err());
        }
    }

    #[test]
    fn unknown_opaque_assertions_are_never_discarded() {
        for schema in [
            json!({"type":"string","format":"binary","minLength":1}),
            json!({"type":"string","format":"binary","title":"File","oneOf":[{}]}),
            json!({"type":"string","format":"binary","title":"Manual","oneOf":[{},{},{},{"type":["string"]}]}),
            json!({"type":"string","format":"binary","title":"File","oneOf":[{},{},{},{"type":["string"]}],"maxLength":1}),
            json!({"type":"string","format":"binary","title":"Manual","anyOf":[{},{},{},{"type":["string"]}]}),
        ] {
            assert!(compile_multipart(&schema).is_err());
        }
    }

    #[test]
    fn the_file_marker_is_opaque_under_one_of_or_any_of() {
        for keyword in ["oneOf", "anyOf"] {
            let schema = json!({"type":"string","format":"binary","title":"File",
                keyword:[{},{},{},{"type":["string"]}]});
            assert!(opaque_binary_schema(&schema), "{keyword}");
            assert!(compile_multipart(&schema).is_ok(), "{keyword}");
        }
    }
}
