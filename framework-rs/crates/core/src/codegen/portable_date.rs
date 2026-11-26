//! Date codecs have a deliberately bounded, explicit initial schema profile.
use anyhow::{Result, bail};
use serde_json::Value;

pub(super) fn check(schema: &Value) -> Result<()> {
    visit(schema, false)
}
fn visit(schema: &Value, required_member: bool) -> Result<()> {
    let Some(object) = schema.as_object() else {
        return Ok(());
    };
    if object.contains_key("x-forklaunch-unbound-native-date") {
        bail!(
            "native chrono date alias requires #[field(codec = DateCodec::UtcMilliseconds)]; implicit date-to-string export is unsupported"
        );
    }
    if let Some(codec) = object.get("x-forklaunch-date-codec")
        && (!required_member
            || object.get("type").and_then(Value::as_str) != Some("string")
            || codec.get("version").and_then(Value::as_u64) != Some(1)
            || codec.get("kind").and_then(Value::as_str) != Some("utc-milliseconds")
            || !matches!(
                codec.get("projection").and_then(Value::as_str),
                Some("value" | "input")
            ))
    {
        bail!(
            "UTC date codec requires a required direct object member and supported metadata; root date values are not yet supported"
        );
    }
    if let Some(fields) = object.get("properties").and_then(Value::as_object) {
        for (name, value) in fields {
            let _ = name;
            visit(value, true)?;
        }
    }
    for key in [
        "$defs",
        "definitions",
        super::schema::DEFINITIONS,
        "patternProperties",
        "dependentSchemas",
    ] {
        if let Some(fields) = object.get(key).and_then(Value::as_object) {
            for value in fields.values() {
                visit(value, false)?;
            }
        }
    }
    for key in [
        "items",
        "additionalItems",
        "additionalProperties",
        "unevaluatedProperties",
        "unevaluatedItems",
        "contains",
        "not",
        "if",
        "then",
        "else",
        "propertyNames",
        "allOf",
        "anyOf",
        "oneOf",
        "prefixItems",
    ] {
        if let Some(value) = object.get(key) {
            if let Some(values) = value.as_array() {
                for value in values {
                    visit(value, required_member)?;
                }
            } else {
                visit(value, required_member)?;
            }
        }
    }
    Ok(())
}

pub(super) fn check_wire(schema: &Value) -> Result<()> {
    if !schema.to_string().contains("x-forklaunch-date-codec") {
        return Ok(());
    }
    let locations = crate::contract::schema_locations(schema, false)?;
    if !locations.iter().any(|location| {
        schema
            .pointer(location)
            .is_some_and(|node| node.get("x-forklaunch-date-codec").is_some())
    }) {
        return Ok(());
    }
    for location in locations {
        let Some(node) = schema.pointer(&location).and_then(Value::as_object) else {
            continue;
        };
        if ["allOf", "if", "then", "else", "dependentSchemas"]
            .iter()
            .any(|key| node.contains_key(*key))
        {
            bail!(
                "Date codec conditional/intersection graphs are not yet supported; use a concrete DTO or discriminated union"
            );
        }
        if node.contains_key("$ref")
            && node.keys().any(|key| {
                !matches!(
                    key.as_str(),
                    "$ref"
                        | "$schema"
                        | "$id"
                        | "$defs"
                        | "definitions"
                        | "title"
                        | "description"
                        | "default"
                        | "examples"
                        | "deprecated"
                        | "readOnly"
                        | "writeOnly"
                )
            })
        {
            bail!("Date codec reference assertion siblings are not yet supported");
        }
    }
    Ok(())
}
