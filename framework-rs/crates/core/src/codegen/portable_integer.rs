//! Known wide native integer formats require an explicit portable wire policy.
use anyhow::{Result, bail};
use serde_json::Value;

pub(super) fn check(schema: &Value) -> Result<()> {
    const SAFE: f64 = 9_007_199_254_740_991.0;
    let mut pending = vec![schema];
    while let Some(node) = pending.pop() {
        let Some(object) = node.as_object() else {
            continue;
        };
        let integer = match object.get("type") {
            Some(Value::String(kind)) => kind == "integer",
            Some(Value::Array(kinds)) => kinds.iter().any(|kind| kind == "integer"),
            _ => false,
        };
        if integer
            && object
                .get("format")
                .and_then(Value::as_str)
                .is_some_and(|format| {
                    matches!(
                        format,
                        "int64" | "uint64" | "int128" | "uint128" | "int" | "uint"
                    )
                })
        {
            let min = object.get("minimum").and_then(Value::as_f64);
            let max = object.get("maximum").and_then(Value::as_f64);
            if !matches!((min,max),(Some(min),Some(max)) if min>=-SAFE && max<=SAFE && min<=max) {
                bail!(
                    "wide native integer schema requires explicit JavaScript-safe minimum/maximum bounds or a supported exact integer wire codec"
                );
            }
        }
        for key in [
            "properties",
            "patternProperties",
            "definitions",
            "$defs",
            "dependentSchemas",
            super::schema::DEFINITIONS,
        ] {
            if let Some(fields) = object.get(key).and_then(Value::as_object) {
                pending.extend(fields.values());
            }
        }
        for key in [
            "additionalProperties",
            "additionalItems",
            "unevaluatedProperties",
            "unevaluatedItems",
            "propertyNames",
            "contains",
            "not",
            "if",
            "then",
            "else",
        ] {
            if let Some(child) = object.get(key) {
                pending.push(child);
            }
        }
        for key in ["allOf", "anyOf", "oneOf", "prefixItems", "items"] {
            match object.get(key) {
                Some(Value::Array(children)) => pending.extend(children),
                Some(child) if key == "items" => pending.push(child),
                _ => {}
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn wide_integers_need_safe_bounds_but_literals_are_not_schemas() {
        for format in ["int64", "uint64", "int128", "uint128", "int", "uint"] {
            assert!(check(&json!({"type":"integer","format":format})).is_err());
            assert!(check(&json!({"type":["integer","null"],"format":format,"minimum":0,"maximum":9_007_199_254_740_991_u64})).is_ok());
            assert!(check(&json!({"type":"integer","format":format,"minimum":0,"maximum":9_007_199_254_740_992_u64})).is_err());
        }
        assert!(check(&json!({"type":"string","format":"int64","examples":[{"type":"integer","format":"int64"}]})).is_ok());
        assert!(
            check(
                &json!({"type":"object","properties":{"id":{"type":"integer","format":"int64"}}})
            )
            .is_err()
        );
    }
}
