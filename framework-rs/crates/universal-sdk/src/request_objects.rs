//! Validate complete declared query/header objects after transport hooks.
use crate::SdkError;
use forklaunch_core::contract::schema_validation::{
    SchemaValidationFailure, compile_checked, decode_wire_field,
};
use serde_json::{Map, Value};
pub(crate) fn validate(request: &reqwest::Request, metadata: &str) -> Result<(), SdkError> {
    let groups: Value = serde_json::from_str(metadata).map_err(|_| invalid())?;
    if matches(request, &groups)? {
        Ok(())
    } else {
        Err(invalid())
    }
}
fn invalid() -> SdkError {
    SdkError::Contract("request fields violate declared object constraints")
}
pub(crate) fn matches(request: &reqwest::Request, groups: &Value) -> Result<bool, SdkError> {
    let (query, headers) = crate::versions::request_fields(request)?;
    for (location, group) in groups.as_object().ok_or_else(invalid)? {
        let is_headers = location == "headers";
        let values = if is_headers {
            &headers
        } else if location == "query" {
            &query
        } else {
            return Err(invalid());
        };
        let rules = group["fields"].as_object().ok_or_else(invalid)?;
        let mut object = Map::new();
        for (name, rule) in rules {
            let key = if is_headers {
                name.to_ascii_lowercase()
            } else {
                name.clone()
            };
            let validator = compile_checked(&rule["schema"]).map_err(|_| invalid())?;
            let decoded = match decode_wire_field(
                validator.as_ref(),
                values.get(&key).map_or(&[], Vec::as_slice),
                rule["required"].as_bool().ok_or_else(invalid)?,
            ) {
                Ok(value) => value,
                Err(SchemaValidationFailure::Invalid(_)) => return Ok(false),
                Err(SchemaValidationFailure::Operational) => return Err(invalid()),
            };
            if let Some(value) = decoded {
                object.insert(name.clone(), value);
            }
        }
        if !is_headers {
            for (name, wire) in values {
                if !rules.contains_key(name) {
                    object.insert(
                        name.clone(),
                        if wire.len() == 1 {
                            Value::String(wire[0].clone())
                        } else {
                            Value::Array(wire.iter().cloned().map(Value::String).collect())
                        },
                    );
                }
            }
        }
        match compile_checked(&group["schema"]).map_err(|_| invalid())?(&Value::Object(object)) {
            Ok(()) => {}
            Err(SchemaValidationFailure::Invalid(_)) => return Ok(false),
            Err(SchemaValidationFailure::Operational) => return Err(invalid()),
        }
    }
    Ok(true)
}
