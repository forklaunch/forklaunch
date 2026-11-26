//! Extracting schema ID.
use serde_json::Value;

pub(crate) fn dollar_id(contents: &Value) -> Option<&str> {
    contents
        .as_object()
        .and_then(|obj| obj.get("$id"))
        .and_then(|id| id.as_str())
}

pub(crate) fn legacy_dollar_id(contents: &Value) -> Option<&str> {
    let object = contents.as_object()?;
    #[cfg(not(feature = "forklaunch-ajv-ref-siblings"))]
    if object.contains_key("$ref") {
        return None;
    }
    if let Some(id) = object.get("$id").and_then(|id| id.as_str()) {
        if !id.starts_with('#') {
            return Some(id);
        }
    }
    None
}

pub(crate) fn legacy_id(contents: &Value) -> Option<&str> {
    let object = contents.as_object()?;
    #[cfg(not(feature = "forklaunch-ajv-ref-siblings"))]
    if object.contains_key("$ref") {
        return None;
    }
    if let Some(id) = object.get("id").and_then(|id| id.as_str()) {
        if !id.starts_with('#') {
            return Some(id);
        }
    }
    None
}

#[cfg(all(test, feature = "forklaunch-ajv-ref-siblings"))]
mod forklaunch_tests {
    use super::{legacy_dollar_id, legacy_id};
    use serde_json::json;
    #[test]
    fn legacy_reference_siblings_establish_ajv_id_scopes() {
        for id_key in ["id", "$id"] {
            let value = json!({id_key:"nested/", "$ref":"target"});
            assert_eq!(
                crate::spec::analyze_object_classic(value.as_object().unwrap(), id_key).id,
                Some("nested/")
            );
        }
        assert_eq!(
            legacy_id(&json!({"id":"nested/", "$ref":"target"})),
            Some("nested/")
        );
        assert_eq!(
            legacy_dollar_id(&json!({"$id":"nested/", "$ref":"target"})),
            Some("nested/")
        );
    }
}
