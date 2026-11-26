//! Portable bounds that Schemars otherwise publishes only as format annotations.
use schemars::{
    JsonSchema, Schema,
    transform::{Transform, transform_subschemas},
};
use serde_json::{Map, Value};

/// Draft-07 root schema, the dialect the contract validators and TypeScript share.
pub(super) fn root<T: JsonSchema>() -> Schema {
    let mut root = schemars::generate::SchemaSettings::draft07()
        .into_generator()
        .into_root_schema_for::<T>();
    SmallIntegerBounds.transform(&mut root);
    if root.as_object().is_some_and(is_object_schema) {
        root.insert(
            "x-forklaunch-model".into(),
            serde_json::json!({
                "version": 1, "id": T::schema_id(), "name": T::schema_name(), "projection": "value"
            }),
        );
    }
    root
}

/// Attach identity only to the named DTO's own object definition.
#[doc(hidden)]
#[must_use]
pub fn named_schema<T: JsonSchema>(mut schema: Schema) -> Schema {
    if schema.as_object().is_some_and(is_object_schema) {
        schema.insert(
            "x-forklaunch-model".into(),
            serde_json::json!({
                "version":1,"id":T::schema_id(),"name":T::schema_name(),"projection":"value"
            }),
        );
    }
    schema
}

/// Generic model names keep the primitive spellings earlier releases published
/// (`Page_for_String`), since they name generated SDK types. Schemars 1 spells
/// unformatted primitives in lower case.
#[doc(hidden)]
#[must_use]
pub fn legacy_schema_name(name: &str) -> String {
    name.split('_')
        .map(|token| match token {
            "string" => "String",
            "boolean" => "Boolean",
            "null" => "Null",
            token => token,
        })
        .collect::<Vec<_>>()
        .join("_")
}

/// Whether a schema carries object validation, as derived structs and maps do.
fn is_object_schema(schema: &Map<String, Value>) -> bool {
    [
        "properties",
        "required",
        "additionalProperties",
        "patternProperties",
        "propertyNames",
        "minProperties",
        "maxProperties",
    ]
    .iter()
    .any(|keyword| schema.contains_key(*keyword))
}

struct SmallIntegerBounds;
impl Transform for SmallIntegerBounds {
    fn transform(&mut self, schema: &mut Schema) {
        if let Some(object) = schema.as_object_mut() {
            // Serde defaults loosen raw input only; canonical encoded values always
            // include these supported scalar members.
            let defaulted: Vec<String> = object
                .get("properties")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
                .filter(|(_, property)| property.get(super::codec::DEFAULT).is_some())
                .map(|(name, _)| name.clone())
                .collect();
            if !defaulted.is_empty() {
                let required = object
                    .entry("required")
                    .or_insert_with(|| Value::Array(Vec::new()));
                if let Some(required) = required.as_array_mut() {
                    for name in defaulted {
                        if !required
                            .iter()
                            .any(|value| value.as_str() == Some(name.as_str()))
                        {
                            required.push(Value::String(name));
                        }
                    }
                }
            }
            integer_bounds(object);
        }
        transform_subschemas(self, schema);
    }
}

// Only integer assertions carry native numeric bounds. A format-looking
// string annotation, enum literal, example or default is never rewritten.
fn integer_bounds(schema: &mut Map<String, Value>) {
    let integer = match schema.get("type") {
        Some(Value::String(kind)) => kind == "integer",
        Some(Value::Array(kinds)) => kinds.iter().any(|kind| kind == "integer"),
        _ => false,
    };
    if !integer {
        return;
    }
    let format = schema
        .get("format")
        .and_then(Value::as_str)
        .map(str::to_owned);
    // Platform-sized and wide unsigned formats have the same lower bound.
    // Intersect rather than replace authored tighter limits; aliases and
    // optional members retain these formats from Schemars.
    let bounds = match format.as_deref() {
        Some("uint" | "uint64" | "uint128") => Some((Some(0), None)),
        Some("uint8") => Some((Some(0), Some(i64::from(u8::MAX)))),
        Some("uint16") => Some((Some(0), Some(i64::from(u16::MAX)))),
        Some("uint32") => Some((Some(0), Some(i64::from(u32::MAX)))),
        Some("int8") => Some((Some(i64::from(i8::MIN)), Some(i64::from(i8::MAX)))),
        Some("int16") => Some((Some(i64::from(i16::MIN)), Some(i64::from(i16::MAX)))),
        Some("int32") => Some((Some(i64::from(i32::MIN)), Some(i64::from(i32::MAX)))),
        _ => None,
    };
    let Some((minimum, maximum)) = bounds else {
        return;
    };
    if let Some(minimum) = minimum {
        tighten(schema, "minimum", minimum, |authored, native| {
            authored >= native
        });
    }
    if let Some(maximum) = maximum {
        tighten(schema, "maximum", maximum, |authored, native| {
            authored <= native
        });
    }
}

/// Keep an authored bound that is already at least as tight; otherwise write
/// the native one.
fn tighten(
    schema: &mut Map<String, Value>,
    keyword: &str,
    bound: i64,
    tighter: fn(f64, f64) -> bool,
) {
    #[allow(clippy::cast_precision_loss)] // Bounds are at most 32-bit.
    let native = bound as f64;
    if schema
        .get(keyword)
        .and_then(Value::as_f64)
        .is_some_and(|authored| tighter(authored, native))
    {
        return;
    }
    schema.insert(keyword.to_owned(), Value::from(bound));
}

/// Schema-only provenance adapter; it never replaces the application's field type.
#[doc(hidden)]
pub struct NativeMember<T: ?Sized>(std::marker::PhantomData<T>);
impl<T: JsonSchema + ?Sized> JsonSchema for NativeMember<T> {
    fn inline_schema() -> bool {
        unbound_date::<T>() || T::inline_schema()
    }
    fn schema_name() -> std::borrow::Cow<'static, str> {
        T::schema_name()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        T::schema_id()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> Schema {
        if unbound_date::<T>() {
            return rejected_date_alias();
        }
        T::json_schema(generator)
    }
    fn _schemars_private_non_optional_json_schema(
        generator: &mut schemars::SchemaGenerator,
    ) -> Schema {
        if unbound_date::<T>() {
            return rejected_date_alias();
        }
        T::_schemars_private_non_optional_json_schema(generator)
    }
    fn _schemars_private_is_option() -> bool {
        T::_schemars_private_is_option()
    }
}
fn unbound_date<T: ?Sized>() -> bool {
    std::any::type_name::<T>().contains(std::any::type_name::<
        super::datetime::DateTime<super::datetime::Utc>,
    >())
}
fn rejected_date_alias() -> Schema {
    schemars::json_schema!({ "not": true, "x-forklaunch-unbound-native-date": true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bounds_preserve_narrower_constraints_and_literal_annotations() {
        let mut root: Schema = serde_json::from_value(json!({
            "type":"integer","format":"int16","minimum":-5,"maximum":7,
            "examples":[{"type":"integer","format":"uint8"}],
            "definitions":{"text":{"type":"string","format":"uint8"}}
        }))
        .unwrap();
        SmallIntegerBounds.transform(&mut root);
        let value = serde_json::to_value(root).unwrap();
        assert_eq!(value["minimum"].as_f64(), Some(-5.0));
        assert_eq!(value["maximum"].as_f64(), Some(7.0));
        assert!(value["definitions"]["text"].get("maximum").is_none());
        assert!(value["examples"][0].get("maximum").is_none());
    }
}
