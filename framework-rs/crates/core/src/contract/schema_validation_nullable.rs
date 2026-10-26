//! `OpenAPI` nullable support at compiled schema locations, matching AJV's profile.
use serde_json::{Map, Value};

pub(super) fn rewrite(schema: &Map<String, Value>) -> Result<Option<Value>, String> {
    let Some(nullable) = schema.get("nullable") else {
        return Ok(None);
    };
    let nullable = nullable.as_bool().ok_or("nullable must be a boolean")?;
    let mut types = match schema.get("type") {
        Some(Value::String(kind)) => vec![Value::String(kind.clone())],
        Some(Value::Array(types)) if !types.is_empty() => types.clone(),
        _ => return Err("nullable requires an explicit schema type".into()),
    };
    let has_null = types.iter().any(|kind| kind.as_str() == Some("null"));
    if has_null && !nullable {
        return Err("type null contradicts nullable false".into());
    }
    if has_null || !nullable {
        return Ok(None);
    }
    // Only type admission changes. Enum, const and composition constraints must
    // still pass; nullable never means an unconditional anyOf with null.
    types.push(Value::String("null".into()));
    let mut rewritten = schema.clone();
    rewritten.insert("type".into(), Value::Array(types));
    rewritten.remove("nullable");
    Ok(Some(Value::Object(rewritten)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::schema_validation::{compile, compile_checked};
    use serde_json::json;

    #[test]
    fn nullable_adds_type_admission_without_bypassing_other_assertions() {
        for schema in [
            json!({"type":"string","nullable":true,"minLength":2}),
            json!({"type":["string"],"nullable":true,"minLength":2}),
        ] {
            let validate = compile_checked(&schema).unwrap();
            assert!(validate(&Value::Null).is_ok());
            assert!(validate(&json!("ok")).is_ok());
            assert!(validate(&json!("x")).is_err());
            assert!(validate(&json!(7)).is_err());
            assert!(compile(&schema).unwrap()(&Value::Null).is_ok());
        }
        for schema in [
            json!({"type":"string","nullable":true,"enum":["ok"]}),
            json!({"type":"string","nullable":true,"const":"ok"}),
            json!({"type":"string","nullable":true,"allOf":[{"type":"string"}]}),
        ] {
            let validate = compile_checked(&schema).unwrap();
            assert!(validate(&Value::Null).is_err());
            assert!(validate(&json!("ok")).is_ok());
        }
        assert!(
            compile_checked(&json!({"type":"string","nullable":true,"enum":["ok",null]})).unwrap()(
                &Value::Null
            )
            .is_ok()
        );
    }

    #[test]
    fn nullable_preserves_referenced_resources_and_literal_schema_shaped_data() {
        let literal = json!({"type":"string","nullable":true});
        let schema = json!({
            "$schema":"https://json-schema.org/draft/2020-12/schema",
            "$id":"https://example.invalid/root", "type":"object", "required":["label","literal"],
            "properties":{"label":{"$ref":"label#value"},"literal":{"const":literal}},
            "$defs":{"Label":{"$id":"label","$anchor":"value","type":"string","nullable":true,"minLength":2}},
            "default":{"type":"string","nullable":true}
        });
        let original = schema.clone();
        let validate = compile_checked(&schema).unwrap();
        assert_eq!(schema, original);
        for label in [Value::Null, json!("ok")] {
            assert!(validate(&json!({"label":label,"literal":literal})).is_ok());
        }
        assert!(validate(&json!({"label":"x","literal":literal})).is_err());
        assert!(validate(&json!({"label":null,"literal":{"type":["string","null"]}})).is_err());
    }

    #[test]
    fn nullable_requires_valid_explicit_consistent_types() {
        for schema in [
            json!({"nullable":true}),
            json!({"nullable":false}),
            json!({"type":"string","nullable":"true"}),
            json!({"type":"null","nullable":false}),
            json!({"type":["string","null"],"nullable":false}),
        ] {
            assert!(compile_checked(&schema).is_err(), "accepted {schema}");
        }
        let validate = compile_checked(&json!({"type":"string","nullable":false})).unwrap();
        assert!(validate(&Value::Null).is_err());
        assert!(validate(&json!("ok")).is_ok());
        assert!(
            compile_checked(&json!({"type":["string","null"],"nullable":true})).unwrap()(
                &Value::Null
            )
            .is_ok()
        );
    }
}
