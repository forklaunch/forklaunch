//! Explicit portable transformations and their request/value schema projections.
use serde_json::{Value, json};

/// Portable normalization operations supported by the schema compiler.
///
/// ```
/// use forklaunch_core::contract::{schema, codec::Normalization};
/// #[schema]
/// struct Input {
///     #[field(wire_max_length=1024, normalize=Normalization::TrimAscii, min_length=1,max_length=200)]
///     title: String,
/// }
/// ```
///
/// ```compile_fail
/// use forklaunch_core::contract::schema;
/// enum Normalization { TrimAscii }
/// #[schema]
/// struct Invalid {
///     #[field(wire_max_length=10, normalize=Normalization::TrimAscii)]
///     title: String,
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Normalization {
    /// Remove only ASCII TAB, LF, VT, FF, CR and SPACE at the two ends.
    TrimAscii,
}

const CODEC: &str = "x-forklaunch-string-codec";
pub(super) const DEFAULT: &str = "x-forklaunch-default-codec";
const ASCII_SPACE: &str = r"[\u0009\u000a\u000b\u000c\u000d\u0020]";

fn trim_ascii(value: &str) -> &str {
    value.trim_matches(['\t', '\n', '\u{b}', '\u{c}', '\r', ' '])
}

/// Decode the bounded raw string, normalize, then validate the application value.
///
/// # Errors
/// Rejects nonstrings, excessive raw length, and invalid normalized length.
#[doc(hidden)]
pub fn deserialize_trim_ascii<
    'de,
    D: serde::Deserializer<'de>,
    const WIRE: usize,
    const MIN: usize,
    const MAX: usize,
>(
    deserializer: D,
) -> Result<String, D::Error> {
    let value = <String as serde::Deserialize>::deserialize(deserializer)?;
    if value.chars().count() > WIRE {
        return Err(serde::de::Error::custom(
            "raw string exceeds contract limit",
        ));
    }
    let normalized = trim_ascii(&value);
    if !(MIN..=MAX).contains(&normalized.chars().count()) {
        return Err(serde::de::Error::custom(
            "normalized string length violates contract",
        ));
    }
    Ok(normalized.into())
}

/// Encode a value that already satisfies the normalized application contract.
///
/// # Errors
/// Rejects unnormalized values, invalid length, and serializer failures.
#[doc(hidden)]
pub fn serialize_trim_ascii<S: serde::Serializer, const MIN: usize, const MAX: usize>(
    value: &str,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    if value != trim_ascii(value) || !(MIN..=MAX).contains(&value.chars().count()) {
        return Err(serde::ser::Error::custom(
            "string violates normalized contract",
        ));
    }
    serializer.serialize_str(value)
}

/// Construct the canonical normalized string schema.
///
/// # Panics
/// Panics only if the framework-owned schema literal is invalid.
#[cfg(feature = "typed-schema")]
#[doc(hidden)]
#[must_use]
pub fn trim_ascii_schema<const WIRE: usize, const MIN: usize, const MAX: usize>(
    _: &mut schemars::SchemaGenerator,
) -> schemars::Schema {
    // Separate start/end prohibitions avoid JavaScript `$` accepting a final LF.
    serde_json::from_value(json!({
        "type":"string", "minLength":MIN, "maxLength":MAX,
        "allOf":[{"not":{"pattern":format!("^{ASCII_SPACE}")}},{"not":{"pattern":format!("{ASCII_SPACE}$")}}],
        (CODEC):{"version":1,"normalization":"trim-ascii","projection":"value","wireMaxLength":WIRE,"minLength":MIN,"maxLength":MAX}
    })).expect("framework string codec schema is valid")
}

/// Construct canonical metadata for a supported Rust Default scalar.
///
/// ```
/// use forklaunch_core::contract::schema;
/// #[schema]
/// struct Input {
///     #[field(default, max_length=200)]
///     body: String,
///     #[field(default, min=0, max=100)]
///     offset: i64,
/// }
/// ```
///
/// ```compile_fail
/// use forklaunch_core::contract::schema;
/// #[schema]
/// struct Invalid { #[field(default, min_length=1)] title: String }
/// ```
///
/// # Panics
/// Panics only if a supported primitive default cannot serialize.
#[cfg(feature = "typed-schema")]
#[doc(hidden)]
#[must_use]
pub fn default_schema<T: schemars::JsonSchema + Default + serde::Serialize>(
    generator: &mut schemars::SchemaGenerator,
) -> schemars::Schema {
    let mut schema = T::json_schema(generator);
    let value = serde_json::to_value(T::default()).expect("supported primitive default serializes");
    schema.insert("default".into(), value.clone());
    schema.insert(
        DEFAULT.into(),
        json!({"version":1,"projection":"value","value":value}),
    );
    schema
}

/// Project canonical normalized-value schemas into accepted raw request schemas.
/// Literal examples/defaults are never traversed. Unsupported codec metadata
/// produces a rejecting schema so no requested transformation is silently ignored.
#[must_use]
pub fn input_schema(schema: &Value) -> Value {
    // Preserve existing schemas (including dialects/resources outside this codec
    // profile) unless the reserved metadata key is present at all.
    if !schema.to_string().contains(CODEC)
        && !schema.to_string().contains(DEFAULT)
        && !schema.to_string().contains("x-forklaunch-date-codec")
    {
        return schema.clone();
    }
    let Ok(locations) = super::schema_locations(schema, false) else {
        return Value::Bool(false);
    };
    let mut input = schema.clone();
    for location in locations {
        let Some(node) = input.pointer_mut(&location).and_then(Value::as_object_mut) else {
            continue;
        };
        let default_fields = node
            .get("properties")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter(|(_, property)| property.get(DEFAULT).is_some())
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        if let Some(required) = node.get_mut("required").and_then(Value::as_array_mut) {
            required.retain(|name| {
                !name
                    .as_str()
                    .is_some_and(|name| default_fields.iter().any(|field| field == name))
            });
        }
        if let Some(default) = node.get(DEFAULT).cloned() {
            if default.get("version") != Some(&json!(1))
                || !matches!(
                    default.get("projection").and_then(Value::as_str),
                    Some("value" | "input")
                )
                || default.get("value") != node.get("default")
            {
                return Value::Bool(false);
            }
            if let Some(metadata) = node.get_mut(DEFAULT).and_then(Value::as_object_mut) {
                metadata.insert("projection".into(), json!("input"));
            }
        }
        if let Some(date) = node.get("x-forklaunch-date-codec").cloned() {
            if date.get("version") != Some(&json!(1))
                || date.get("kind").and_then(Value::as_str) != Some("utc-milliseconds")
                || !matches!(
                    date.get("projection").and_then(Value::as_str),
                    Some("value" | "input")
                )
            {
                return Value::Bool(false);
            }
            node.insert(
                "pattern".into(),
                json!(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-5][0-9](\.[0-9]{1,3})?Z$"),
            );
            if let Some(metadata) = node.get_mut("x-forklaunch-date-codec") {
                metadata["projection"] = json!("input");
            }
        }
        let Some(codec) = node.get(CODEC).cloned() else {
            continue;
        };
        if codec.get("version") != Some(&json!(1))
            || codec.get("normalization").and_then(Value::as_str) != Some("trim-ascii")
            || codec.get("wireMaxLength").and_then(Value::as_u64).is_none()
            || node.get("type").and_then(Value::as_str) != Some("string")
        {
            return Value::Bool(false);
        }
        if codec.get("projection").and_then(Value::as_str) == Some("input") {
            continue;
        }
        let expected_rules = json!([
            {"not":{"pattern":format!("^{ASCII_SPACE}")}},
            {"not":{"pattern":format!("{ASCII_SPACE}$")}}
        ]);
        if codec.get("projection").and_then(Value::as_str) != Some("value")
            || node.get("allOf") != Some(&expected_rules)
            || node.get("minLength") != codec.get("minLength")
            || node.get("maxLength") != codec.get("maxLength")
        {
            return Value::Bool(false);
        }
        if let Some(metadata) = node.get_mut(CODEC).and_then(Value::as_object_mut) {
            metadata.insert("projection".into(), json!("input"));
        }
        node.remove("minLength");
        node.remove("allOf");
        node.insert("maxLength".into(), codec["wireMaxLength"].clone());
    }
    if input != *schema {
        mark_input_identities(&mut input);
    }
    input
}

fn mark_input_identities(input: &mut Value) {
    // Referenced DTOs can change through child definitions even when the
    // containing node has no local codec. Keep the entire graph directional.
    let Ok(locations) = super::schema_locations(input, false) else {
        return;
    };
    for location in locations {
        if let Some(metadata) = input
            .pointer_mut(&location)
            .and_then(|node| node.get_mut("x-forklaunch-model"))
            .and_then(Value::as_object_mut)
        {
            metadata.insert("projection".into(), json!("input"));
        }
    }
}
