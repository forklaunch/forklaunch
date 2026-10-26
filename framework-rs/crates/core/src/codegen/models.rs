//! Shared schema-to-model compiler used by native generators and the packaged CLI.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use convert_case::{Case, Casing};
use ramhorns::Content;
use serde_json::Value as JsonValue;

// This source is also compiled inside the CLI SDK module.
pub(super) const MODEL_IDENTITY_KEY: &str = "x-forklaunch-model";

#[derive(Content, Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct RustModelTemplate {
    pub identity_name: String,
    pub name: String,
    pub fields: Vec<RustModelFieldTemplate>,
    pub union: bool,
    /// Fixed-length JSON sequence, rendered with an explicit Serde visitor.
    pub sequence: bool,
    /// Reachable prefix element count, used for required-rest/default analysis.
    pub sequence_len: usize,
    pub sequence_min: u64,
    pub sequence_max: u64,
    pub sequence_rest: bool,
    pub sequence_rest_type: String,
    pub deny_unknown_fields: bool,
    pub transparent: bool,
    pub has_default: bool,
    pub variants: Vec<RustUnionVariantTemplate>,
    /// A closed set of string values, rendered as a fieldless enum with serde renames.
    pub string_enum: bool,
    pub enum_variants: Vec<RustEnumVariantTemplate>,
}

#[derive(Content, Debug, Clone, PartialEq, Eq)]
pub struct RustEnumVariantTemplate {
    pub name: String,
    pub wire_name_literal: String,
    pub first: bool,
}

#[derive(Content, Debug, Clone, PartialEq, Eq)]
pub struct RustUnionVariantTemplate {
    pub name: String,
    pub value_type: String,
    pub first: bool,
}

#[derive(Content, Debug, Clone, PartialEq, Eq)]
pub struct RustModelFieldTemplate {
    pub date_codec: String,
    pub optional: bool,
    pub required: bool,
    pub flatten: bool,
    pub wire_name_literal: String,
    pub field_name: String,
    pub field_type: String,
}

#[derive(Content, Debug, Clone, PartialEq, Eq)]
pub struct RustTypeAliasTemplate {
    pub name: String,
    pub target_type: String,
}

pub(crate) fn ts_type_from_schema_name(name: &str) -> String {
    match name.to_ascii_lowercase().as_str() {
        "string" | "str" | "uuid" | "datetime" | "date" => "string".to_string(),
        "bool" | "boolean" => "boolean".to_string(),
        "usize" | "u64" | "u32" | "i64" | "i32" | "integer" | "int" | "number" | "float"
        | "f64" | "f32" => "number".to_string(),
        _ => "unknown".to_string(),
    }
}

pub(crate) fn ts_type_from_schema(schema: &JsonValue) -> String {
    if let Some(allowed) = schema.as_bool() {
        return if allowed { "unknown" } else { "never" }.into();
    }
    if let Some(parts) = schema.get("allOf").and_then(JsonValue::as_array) {
        let mut base = schema.clone();
        base.as_object_mut().unwrap().remove("allOf");
        return std::iter::once(&base)
            .chain(parts)
            .map(|part| format!("({})", ts_type_from_schema(part)))
            .collect::<Vec<_>>()
            .join(" & ");
    }
    if let Some(name) = schema
        .get(super::schema::REFERENCE)
        .and_then(JsonValue::as_str)
    {
        return name.to_owned();
    }
    if schema.get("nullable") == Some(&JsonValue::Bool(true)) {
        let mut value = schema.clone();
        value.as_object_mut().unwrap().remove("nullable");
        return format!("{} | null", ts_type_from_schema(&value));
    }
    if let Some(const_value) = schema.get("const") {
        return ts_literal_type(const_value);
    }

    if let Some(enum_values) = schema.get("enum").and_then(JsonValue::as_array)
        && !enum_values.is_empty()
    {
        return enum_values
            .iter()
            .map(ts_literal_type)
            .collect::<Vec<_>>()
            .join(" | ");
    }

    if let Some(one_of) = schema
        .get("oneOf")
        .or_else(|| schema.get("anyOf"))
        .and_then(JsonValue::as_array)
        && !one_of.is_empty()
    {
        return one_of
            .iter()
            .map(ts_type_from_schema)
            .collect::<Vec<_>>()
            .join(" | ");
    }

    if let Some(types) = schema.get("type").and_then(JsonValue::as_array) {
        return types
            .iter()
            .filter_map(JsonValue::as_str)
            .map(|kind| {
                let mut variant = schema.clone();
                variant["type"] = JsonValue::String(kind.to_owned());
                ts_type_from_schema(&variant)
            })
            .collect::<Vec<_>>()
            .join(" | ");
    }

    match schema_type(schema) {
        Some("object") => ts_object_type(schema),
        Some("array") => ts_array_type(schema),
        Some("integer" | "number") => "number".to_string(),
        Some("string") if schema.get("x-forklaunch-date-codec").is_some() => "Date".to_string(),
        Some("string") => "string".to_string(),
        Some("boolean") => "boolean".to_string(),
        Some("null") => "null".to_string(),
        _ if schema.get("properties").is_some() => ts_object_type(schema),
        _ => "unknown".to_string(),
    }
}

fn ts_array_type(schema: &JsonValue) -> String {
    if let Some(prefix) = schema
        .get("prefixItems")
        .or_else(|| schema.get("items").filter(|items| items.is_array()))
        .and_then(JsonValue::as_array)
    {
        let tail = if schema.get("prefixItems").is_some() {
            schema.get("items")
        } else {
            schema.get("additionalItems")
        };
        let minimum = schema
            .get("minItems")
            .and_then(JsonValue::as_u64)
            .unwrap_or(0);
        let maximum = schema
            .get("maxItems")
            .and_then(JsonValue::as_u64)
            .unwrap_or(u64::MAX);
        let mut fields = prefix
            .iter()
            .enumerate()
            .take_while(|(index, _)| (*index as u64) < maximum)
            .map(|(index, item)| {
                format!(
                    "item{index}{}: {}",
                    if (index as u64) < minimum { "" } else { "?" },
                    ts_type_from_schema(item)
                )
            })
            .collect::<Vec<_>>();
        if maximum > prefix.len() as u64 && tail != Some(&JsonValue::Bool(false)) {
            fields.push(format!(
                "...rest: ({})[]",
                tail.map_or_else(|| "unknown".into(), ts_type_from_schema)
            ));
        }
        return format!("[{}]", fields.join(", "));
    }
    let item_type = schema
        .get("items")
        .map_or_else(|| "unknown".to_string(), ts_type_from_schema);
    format!("Array<{item_type}>")
}

pub(crate) fn ts_object_type(schema: &JsonValue) -> String {
    let properties = schema.get("properties").and_then(JsonValue::as_object);
    if properties.is_none_or(serde_json::Map::is_empty) {
        let value_type = match schema.get("additionalProperties") {
            Some(JsonValue::Bool(false)) => "never".into(),
            Some(value) if value.is_object() => ts_type_from_schema(value),
            _ => "unknown".into(),
        };
        if schema
            .get("additionalProperties")
            .and_then(|value| value.get(super::schema::REFERENCE))
            .is_some()
        {
            return format!("{{ [key: string]: {value_type} }}");
        }
        return format!("Record<string, {value_type}>");
    }
    let properties = properties.unwrap();

    let required = schema
        .get("required")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .filter_map(JsonValue::as_str)
        .collect::<BTreeSet<_>>();

    let mut parts = Vec::new();
    for (name, property_schema) in properties {
        let optional = if required.contains(name.as_str()) {
            ""
        } else {
            "?"
        };
        parts.push(format!(
            "{}{}: {}",
            ts_property_key(name),
            optional,
            ts_type_from_schema(property_schema)
        ));
    }

    let object_type = format!("{{ {} }}", parts.join("; "));
    // An ordinary DTO exposes its declared members. JSON Schema's implicit
    // permission to receive extra wire properties is not an authored dictionary
    // API: adding an index signature widens `keyof` and admits misspelled keys.
    // Explicit open objects and dictionaries retain their dynamic-key surface.
    if schema
        .get("additionalProperties")
        .is_some_and(|value| value != &JsonValue::Bool(false))
    {
        format!("{object_type} & Record<string, unknown>")
    } else {
        object_type
    }
}

pub(crate) fn schema_type(schema: &JsonValue) -> Option<&str> {
    schema.get("type").and_then(|schema_type| {
        schema_type.as_str().or_else(|| {
            schema_type.as_array().and_then(|types| {
                types
                    .iter()
                    .filter_map(JsonValue::as_str)
                    .find(|ty| *ty != "null")
            })
        })
    })
}

pub(crate) fn ts_literal_type(value: &JsonValue) -> String {
    if let Some(value) = value.as_str() {
        return format!("'{}'", escape_ts_string(value));
    }
    if value.is_number() || value.is_boolean() {
        return value.to_string();
    }
    if value.is_null() {
        return "null".to_string();
    }
    match value {
        JsonValue::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(ts_literal_type)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        JsonValue::Object(values) if values.is_empty() => "Record<string, never>".into(),
        JsonValue::Object(values) => format!(
            "{{ {} }}",
            values
                .iter()
                .map(|(key, value)| format!("{}: {}", ts_property_key(key), ts_literal_type(value)))
                .collect::<Vec<_>>()
                .join("; ")
        ),
        _ => "unknown".into(),
    }
}

pub(crate) fn ts_property_key(name: &str) -> String {
    if is_ts_identifier(name) {
        name.to_string()
    } else {
        format!("'{}'", escape_ts_string(name))
    }
}

pub(crate) fn is_ts_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
}

pub(crate) fn rust_types_from_schema(
    root_name: &str,
    schema: &JsonValue,
) -> (Vec<RustModelTemplate>, Vec<RustTypeAliasTemplate>, String) {
    let mut models = BTreeMap::new();
    if let Some(definitions) = schema
        .get(super::schema::DEFINITIONS)
        .and_then(JsonValue::as_object)
    {
        for (name, definition) in definitions {
            let field_type = build_rust_type(name, definition, &mut models);
            if field_type != *name {
                models.insert(
                    name.clone(),
                    RustModelTemplate {
                        identity_name: String::new(),
                        name: name.clone(),
                        fields: vec![RustModelFieldTemplate {
                            date_codec: String::new(),
                            optional: false,
                            required: false,
                            flatten: false,
                            wire_name_literal: "\"value\"".into(),
                            field_name: "value".into(),
                            field_type,
                        }],
                        union: false,
                        sequence: false,
                        sequence_len: 0,
                        sequence_min: 0,
                        sequence_max: 0,
                        sequence_rest: false,
                        sequence_rest_type: String::new(),
                        deny_unknown_fields: false,
                        transparent: true,
                        has_default: true,
                        variants: vec![],
                        string_enum: false,
                        enum_variants: vec![],
                    },
                );
            }
        }
    }
    let response_type = build_rust_type(root_name, schema, &mut models);
    // Deriving Default across required recursive edges would recurse forever.

    let mut defaults = BTreeMap::new();
    for name in models.keys() {
        can_default(name, &models, &mut BTreeSet::new(), &mut defaults);
    }
    for (name, model) in &mut models {
        model.has_default = defaults[name];
    }
    let mut aliases = Vec::new();

    if !models.contains_key(&response_type) {
        aliases.push(RustTypeAliasTemplate {
            name: rust_type_name(root_name),
            target_type: response_type.clone(),
        });
    }

    (
        models.into_values().collect(),
        aliases,
        rust_type_name(root_name),
    )
}

// Strip only a representable nullable alternative; other unions retain JSON values.
pub(crate) fn non_null_schema(schema: &JsonValue) -> Option<JsonValue> {
    let mut variant = schema.clone();
    let mut nullable = variant.get("nullable") == Some(&JsonValue::Bool(true));
    variant.as_object_mut()?.remove("nullable");
    if let Some(types) = variant.get("type").and_then(JsonValue::as_array) {
        let non_null = types
            .iter()
            .filter(|value| value.as_str() != Some("null"))
            .cloned()
            .collect::<Vec<_>>();
        if non_null.len() != types.len() && !non_null.is_empty() {
            nullable = true;
            variant["type"] = if non_null.len() == 1 {
                non_null[0].clone()
            } else {
                JsonValue::Array(non_null)
            };
        }
    }
    for key in ["anyOf", "oneOf"] {
        if let Some(alternatives) = variant.get(key).and_then(JsonValue::as_array)
            && alternatives.len() == 2
            && alternatives
                .iter()
                .any(|value| value.get("type").and_then(JsonValue::as_str) == Some("null"))
        {
            return alternatives
                .iter()
                .find(|value| value.get("type").and_then(JsonValue::as_str) != Some("null"))
                .cloned();
        }
    }
    if nullable {
        // The identity describes the complete nullable model. Its synthesized
        // non-null payload is a different type; an explicit identity on an
        // anyOf/oneOf child above still belongs to that child and is retained.
        variant.as_object_mut()?.remove(MODEL_IDENTITY_KEY);
        Some(variant)
    } else {
        None
    }
}

fn reference_type(name: &str, schema: &JsonValue) -> String {
    if schema.get("x-forklaunch-sdk-inline-model") == Some(&JsonValue::Bool(true)) {
        name.into()
    } else {
        format!("::std::boxed::Box<{name}>")
    }
}

pub(crate) fn build_rust_type(
    type_name: &str,
    schema: &JsonValue,
    models: &mut BTreeMap<String, RustModelTemplate>,
) -> String {
    if let Some(name) = schema
        .get(super::schema::REFERENCE)
        .and_then(JsonValue::as_str)
    {
        return reference_type(name, schema);
    }
    // An intersection can narrow its constituent type but cannot admit a
    // different value shape. Retain a concrete referenced/typed constituent
    // when the containing schema contributes validation-only constraints.
    // The wire codec still evaluates every intersection constraint.
    if !["type", "properties", "const", "enum", "oneOf", "anyOf"]
        .iter()
        .any(|key| schema.get(*key).is_some())
        && let Some(parts) = schema.get("allOf").and_then(JsonValue::as_array)
        && let Some(part) = parts
            .iter()
            .find(|part| part.get(super::schema::REFERENCE).is_some() || part.get("type").is_some())
    {
        return build_rust_type(type_name, part, models);
    }
    if let Some(variant) = non_null_schema(schema) {
        return format!(
            "::std::option::Option<{}>",
            build_rust_type(&format!("{type_name}Value"), &variant, models)
        );
    }
    if let Some(alternatives) = schema
        .get("oneOf")
        .or_else(|| schema.get("anyOf"))
        .and_then(JsonValue::as_array)
    {
        return build_rust_union(type_name, alternatives, models);
    }
    if let Some(types) = schema.get("type").and_then(JsonValue::as_array)
        && types.len() > 1
    {
        let alternatives = types
            .iter()
            .map(|kind| {
                let mut variant = schema.clone();
                variant["type"] = kind.clone();
                // Each synthesized branch projects part of the named union;
                // it must not alias its payload back to the entire union.
                variant.as_object_mut().unwrap().remove(MODEL_IDENTITY_KEY);
                variant
            })
            .collect::<Vec<_>>();
        return build_rust_union(type_name, &alternatives, models);
    }
    if let Some(literal) = literal_rust_type(type_name, schema, models) {
        return literal;
    }

    match schema_type(schema) {
        Some("object")
            if schema
                .get("properties")
                .and_then(JsonValue::as_object)
                .is_none_or(serde_json::Map::is_empty) =>
        {
            let value_type = schema
                .get("additionalProperties")
                .filter(|value| value.is_object())
                .map_or_else(
                    || "::serde_json::Value".into(),
                    |value| build_rust_type(&format!("{type_name}Value"), value, models),
                );
            format!("::std::collections::BTreeMap<::std::string::String, {value_type}>")
        }
        Some("object") => rust_model_from_schema(type_name, schema, models),
        Some("array") => {
            if let Some(shape) = sequence_shape(schema) {
                return build_rust_sequence(type_name, &shape, models);
            }
            let item_type_name = format!("{}Item", rust_type_name(type_name));
            let item_type = schema.get("items").map_or_else(
                || "::serde_json::Value".to_string(),
                |items| build_rust_type(&item_type_name, items, models),
            );
            let item_type = item_type
                .strip_prefix("::std::boxed::Box<")
                .and_then(|value| value.strip_suffix('>'))
                .unwrap_or(&item_type);
            if date_shape(schema) && let Some(length)=schema.get("minItems").and_then(JsonValue::as_u64) && schema.get("maxItems").and_then(JsonValue::as_u64)==Some(length) {
                return format!("[{item_type}; {length}]");
            }
            format!("::std::vec::Vec<{item_type}>")
        }
        Some("integer") => integer_rust_type(schema).to_string(),
        Some("number") => "f64".to_string(),
        Some("string") if schema.get("x-forklaunch-date-codec").is_some() => "::forklaunch_universal_sdk::datetime::DateTime<::forklaunch_universal_sdk::datetime::Utc>".to_string(),
        Some("string") => "::std::string::String".to_string(),
        Some("boolean") => "bool".to_string(),
        Some("null") => "()".to_string(),
        _ if schema.get("properties").is_some() => {
            rust_model_from_schema(type_name, schema, models)
        }
        _ => "::serde_json::Value".to_string(),
    }
}

/// JSON Schema formats alone do not constrain the admitted numeric range.
/// Narrow unsigned native projections only when their actual bounds agree.
fn integer_rust_type(schema: &JsonValue) -> &'static str {
    let Some(minimum) = schema.get("minimum").and_then(JsonValue::as_f64) else {
        return "i64";
    };
    if minimum < 0.0 {
        return "i64";
    }
    let maximum = schema.get("maximum").and_then(JsonValue::as_f64);
    match schema.get("format").and_then(JsonValue::as_str) {
        Some("uint8") if maximum.is_some_and(|max| max <= f64::from(u8::MAX)) => "u8",
        Some("uint16") if maximum.is_some_and(|max| max <= f64::from(u16::MAX)) => "u16",
        Some("uint32") if maximum.is_some_and(|max| max <= f64::from(u32::MAX)) => "u32",
        // `uint` is usize: use a fixed client width across compilation targets.
        Some("uint" | "uint8" | "uint16" | "uint32" | "uint64" | "uint128") => "u64",
        _ => "i64",
    }
}

/// Closed value sets: a string `enum` becomes a Rust enum, a `const` its scalar type.
fn literal_rust_type(
    type_name: &str,
    schema: &JsonValue,
    models: &mut BTreeMap<String, RustModelTemplate>,
) -> Option<String> {
    if let Some(values) = string_enum_values(schema) {
        return Some(build_rust_string_enum(type_name, &values, models));
    }
    let value = schema.get("const")?;
    Some(
        match value {
            JsonValue::Null => "()",
            JsonValue::Bool(_) => "bool",
            JsonValue::Number(number) if number.is_i64() => "i64",
            JsonValue::Number(_) => "f64",
            JsonValue::String(_) => "::std::string::String",
            _ => "::serde_json::Value",
        }
        .into(),
    )
}

/// String values of a closed `enum` (`type: string` or untyped), if every member is a string.
fn string_enum_values(schema: &JsonValue) -> Option<Vec<String>> {
    if !matches!(schema_type(schema), None | Some("string"))
        || schema.get("x-forklaunch-date-codec").is_some()
    {
        return None;
    }
    let values = schema.get("enum")?.as_array()?;
    if values.is_empty() {
        return None;
    }
    let mut seen = BTreeSet::new();
    values
        .iter()
        .map(|value| value.as_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()
        .map(|values| {
            values
                .into_iter()
                .filter(|value| seen.insert(value.clone()))
                .collect()
        })
}

/// A fieldless Rust enum whose variants serialize to the exact wire strings.
/// Identical value sets generated for the same name are shared.
pub(crate) fn build_rust_string_enum(
    type_name: &str,
    values: &[String],
    models: &mut BTreeMap<String, RustModelTemplate>,
) -> String {
    let mut used = BTreeSet::new();
    let enum_variants = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let base = rust_type_name(value);
            let base = if base == "GeneratedModel" || base.ends_with('_') {
                format!("Value{}", index + 1)
            } else {
                base
            };
            let mut name = base.clone();
            let mut suffix = 2;
            while !used.insert(name.clone()) {
                name = format!("{base}{suffix}");
                suffix += 1;
            }
            RustEnumVariantTemplate {
                name,
                wire_name_literal: format!("{value:?}"),
                first: index == 0,
            }
        })
        .collect::<Vec<_>>();
    let base = rust_type_name(type_name);
    let mut name = base.clone();
    let mut suffix = 2;
    while let Some(existing) = models.get(&name) {
        if existing.string_enum && existing.enum_variants == enum_variants {
            return name;
        }
        name = format!("{base}{suffix}");
        suffix += 1;
    }
    let mut model = empty_model(&name);
    model.string_enum = true;
    model.has_default = true;
    model.enum_variants = enum_variants;
    models.insert(name.clone(), model);
    name
}

pub(crate) fn build_rust_union(
    type_name: &str,
    alternatives: &[JsonValue],
    models: &mut BTreeMap<String, RustModelTemplate>,
) -> String {
    if alternatives.is_empty() {
        return "::serde_json::Value".into();
    }
    let base = rust_type_name(type_name);
    let mut name = base.clone();
    let mut suffix = 2;
    while models.contains_key(&name) {
        name = format!("{base}{suffix}");
        suffix += 1;
    }
    models.insert(
        name.clone(),
        RustModelTemplate {
            identity_name: String::new(),
            name: name.clone(),
            fields: vec![],
            union: true,
            sequence: false,
            sequence_len: 0,
            sequence_min: 0,
            sequence_max: 0,
            sequence_rest: false,
            sequence_rest_type: String::new(),
            deny_unknown_fields: false,
            transparent: false,
            has_default: true,
            variants: vec![],
            string_enum: false,
            enum_variants: vec![],
        },
    );
    let variants = alternatives
        .iter()
        .enumerate()
        .map(|(index, schema)| RustUnionVariantTemplate {
            name: format!("Variant{}", index + 1),
            value_type: build_rust_type(&format!("{name}Variant{}", index + 1), schema, models),
            first: index == 0,
        })
        .collect();
    models.get_mut(&name).unwrap().variants = variants;
    name
}

fn empty_model(model_name: &str) -> RustModelTemplate {
    RustModelTemplate {
        identity_name: String::new(),
        name: model_name.to_owned(),
        fields: vec![],
        union: false,
        sequence: false,
        sequence_len: 0,
        sequence_min: 0,
        sequence_max: 0,
        sequence_rest: false,
        sequence_rest_type: String::new(),
        deny_unknown_fields: false,
        transparent: false,
        has_default: true,
        variants: vec![],
        string_enum: false,
        enum_variants: vec![],
    }
}

pub(crate) fn rust_model_from_schema(
    type_name: &str,
    schema: &JsonValue,
    models: &mut BTreeMap<String, RustModelTemplate>,
) -> String {
    let base_name = rust_type_name(type_name);
    let mut model_name = base_name.clone();
    let mut suffix = 2;
    while models.contains_key(&model_name) {
        model_name = format!("{base_name}{suffix}");
        suffix += 1;
    }
    // Reserve the name before nested fields allocate their own models.
    models.insert(model_name.clone(), empty_model(&model_name));

    let required = schema
        .get("required")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .filter_map(JsonValue::as_str)
        .collect::<BTreeSet<_>>();
    let mut fields = Vec::new();
    let mut field_names = BTreeSet::new();

    if let Some(properties) = schema.get("properties").and_then(JsonValue::as_object) {
        for (name, property_schema) in properties {
            let base_field_name = rust_field_ident(name);
            let mut field_name = base_field_name.clone();
            let mut suffix = 2;
            while !field_names.insert(field_name.clone()) {
                field_name = format!("{}_{suffix}", base_field_name.trim_end_matches('_'));
                suffix += 1;
            }
            let nested_type_name = format!("{}{}", model_name, field_name.to_case(Case::Pascal));
            let mut field_type = build_rust_type(&nested_type_name, property_schema, models);
            if !required.contains(name.as_str()) {
                field_type = format!("::std::option::Option<{field_type}>");
            }
            fields.push(RustModelFieldTemplate {
                date_codec: if date_shape(property_schema) {
                    if required.contains(name.as_str()) {
                        "::forklaunch_universal_sdk::datetime".into()
                    } else {
                        "::forklaunch_universal_sdk::datetime::optional".into()
                    }
                } else {
                    String::new()
                },
                optional: !required.contains(name.as_str()),
                required: required.contains(name.as_str()),
                flatten: false,
                wire_name_literal: format!("{name:?}"),
                field_name,
                field_type,
            });
        }
    }

    if schema.get("additionalProperties") != Some(&JsonValue::Bool(false)) {
        let mut field_name = "additional_properties".to_string();
        let mut suffix = 2;
        while !field_names.insert(field_name.clone()) {
            field_name = format!("additional_properties_{suffix}");
            suffix += 1;
        }
        let value_type = schema
            .get("additionalProperties")
            .filter(|value| value.is_object())
            .map_or_else(
                || "::serde_json::Value".into(),
                |value| build_rust_type(&format!("{model_name}AdditionalValue"), value, models),
            );
        fields.push(RustModelFieldTemplate {
            date_codec: String::new(),
            optional: false,
            required: false,
            flatten: true,
            wire_name_literal: "\"\"".into(),
            field_name,
            field_type: format!(
                "::std::collections::BTreeMap<::std::string::String, {value_type}>"
            ),
        });
    }

    models.insert(
        model_name.clone(),
        RustModelTemplate {
            identity_name: forklaunch_core::codegen::model_identity(schema)
                .ok()
                .flatten()
                .map_or_else(String::new, |(_, name)| name),
            fields,
            deny_unknown_fields: schema.get("additionalProperties")
                == Some(&JsonValue::Bool(false)),
            ..empty_model(&model_name)
        },
    );
    model_name
}

static ANY_SEQUENCE_ITEM: JsonValue = JsonValue::Bool(true);

struct SequenceShape<'a> {
    prefix: &'a [JsonValue],
    minimum: u64,
    maximum: u64,
    // Some(true) is an unrestricted JSON rest; None means no rest is admitted.
    rest: Option<&'a JsonValue>,
}

fn sequence_shape(schema: &JsonValue) -> Option<SequenceShape<'_>> {
    let prefix = schema
        .get("prefixItems")
        .or_else(|| schema.get("items").filter(|items| items.is_array()))
        .and_then(JsonValue::as_array);
    let tail = if schema.get("prefixItems").is_some() {
        schema.get("items")
    } else if prefix.is_some() {
        schema.get("additionalItems")
    } else {
        schema.get("items")
    };
    let minimum = match schema.get("minItems") {
        Some(value) => value.as_u64()?,
        None => 0,
    };
    let mut maximum = match schema.get("maxItems") {
        Some(value) => value.as_u64()?,
        None => u64::MAX,
    };
    if tail == Some(&JsonValue::Bool(false)) {
        maximum = maximum.min(prefix.map_or(0, Vec::len) as u64);
    }
    if minimum > maximum || (prefix.is_none() && maximum != 0) {
        return None;
    }
    let prefix = prefix.map_or(&[][..], Vec::as_slice);
    let reachable = prefix
        .len()
        .min(usize::try_from(maximum).unwrap_or(usize::MAX));
    let rest = (maximum > prefix.len() as u64).then(|| tail.unwrap_or(&ANY_SEQUENCE_ITEM));
    Some(SequenceShape {
        prefix: &prefix[..reachable],
        minimum,
        maximum,
        rest,
    })
}

fn build_rust_sequence(
    type_name: &str,
    shape: &SequenceShape<'_>,
    models: &mut BTreeMap<String, RustModelTemplate>,
) -> String {
    let base = rust_type_name(type_name);
    let mut name = base.clone();
    let mut suffix = 2;
    while models.contains_key(&name) {
        name = format!("{base}{suffix}");
        suffix += 1;
    }
    models.insert(
        name.clone(),
        RustModelTemplate {
            identity_name: String::new(),
            name: name.clone(),
            fields: vec![],
            union: false,
            sequence: true,
            sequence_len: shape.prefix.len(),
            sequence_min: shape.minimum,
            sequence_max: shape.maximum,
            sequence_rest: shape.rest.is_some(),
            sequence_rest_type: String::new(),
            deny_unknown_fields: false,
            transparent: false,
            has_default: true,
            variants: vec![],
            string_enum: false,
            enum_variants: vec![],
        },
    );
    let fields = shape
        .prefix
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let optional = index as u64 >= shape.minimum;
            let value = build_rust_type(&format!("{name}Item{index}"), item, models);
            RustModelFieldTemplate {
                date_codec: String::new(),
                optional,
                required: false,
                flatten: false,
                wire_name_literal: format!("\"{index}\""),
                field_name: format!("item{index}"),
                field_type: if optional {
                    format!("::std::option::Option<{value}>")
                } else {
                    value
                },
            }
        })
        .collect();
    let rest = shape
        .rest
        .map(|schema| build_rust_type(&format!("{name}Rest"), schema, models));
    let model = models.get_mut(&name).unwrap();
    model.fields = fields;
    model.sequence_rest_type = rest
        .map(|ty| {
            ty.strip_prefix("::std::boxed::Box<")
                .and_then(|value| value.strip_suffix('>'))
                .unwrap_or(&ty)
                .to_owned()
        })
        .unwrap_or_default();
    name
}

pub(crate) fn rust_type_name(input: &str) -> String {
    let mut name = input
        .to_case(Case::Pascal)
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>();
    if name.is_empty() {
        name = "GeneratedModel".to_string();
    }
    if name.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        name.insert_str(0, "Model");
    }
    // Pascal casing can still produce the reserved `Self` type keyword.
    // A trailing suffix is stable under repeated normalization and cannot
    // collide with an ordinary alphanumeric normalized model name.
    if is_rust_keyword(&name) {
        name.push('_');
    }
    name
}

pub(crate) fn rust_field_ident(input: &str) -> String {
    let mut name = input
        .to_case(Case::Snake)
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if name.is_empty() || name.chars().all(|ch| ch == '_') {
        name = "field".to_string();
    }
    if name.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        name.insert(0, '_');
    }
    if is_rust_keyword(&name) {
        name.push('_');
    }
    name
}

pub(crate) fn is_rust_keyword(input: &str) -> bool {
    matches!(
        input,
        "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "gen"
            | "macro"
            | "override"
            | "priv"
            | "try"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
            | "union"
            | "as"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "async"
            | "await"
            | "dyn"
    )
}

pub(crate) fn escape_ts_string(input: &str) -> String {
    let mut escaped = String::new();
    for ch in input.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '\'' => escaped.push_str("\\'"),
            '\u{0}'..='\u{1f}' | '\u{2028}' | '\u{2029}' => {
                let _ = write!(escaped, "\\u{:04x}", u32::from(ch));
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn can_default(
    name: &str,
    models: &BTreeMap<String, RustModelTemplate>,
    active: &mut BTreeSet<String>,
    memo: &mut BTreeMap<String, bool>,
) -> bool {
    if let Some(result) = memo.get(name) {
        return *result;
    }
    if active.len() >= 64 || !active.insert(name.into()) {
        return false;
    }
    let model = &models[name];
    if model.sequence && model.sequence_min > model.sequence_len as u64 {
        active.remove(name);
        memo.insert(name.into(), false);
        return false;
    }
    let fields = if model.union {
        model
            .variants
            .first()
            .map(|variant| variant.value_type.as_str())
            .into_iter()
            .collect::<Vec<_>>()
    } else {
        model
            .fields
            .iter()
            .map(|field| field.field_type.as_str())
            .collect()
    };
    let result = fields.into_iter().all(|ty| {
        let ty = ty
            .strip_prefix("::std::boxed::Box<")
            .and_then(|value| value.strip_suffix('>'))
            .unwrap_or(ty);
        if let Some(array) = ty.strip_prefix('[').and_then(|ty| ty.strip_suffix(']')) {
            return array
                .rsplit_once(';')
                .and_then(|(_, length)| length.trim().parse::<usize>().ok())
                .is_some_and(|length| length <= 32)
                && !array.starts_with('[');
        }
        !models.contains_key(ty) || can_default(ty, models, active, memo)
    });
    active.remove(name);
    memo.insert(name.into(), result);
    result
}

#[derive(Content)]
struct ModelSourceTemplate<'a> {
    has_optional_fields: bool,
    has_required_fields: bool,
    models: &'a [RustModelTemplate],
    aliases: &'a [RustTypeAliasTemplate],
}

/// Render Serde models, preserving wire field names and omission/null presence.
/// Generated builtins and Serde paths are absolute; no caller imports are required.
///
/// # Errors
/// Returns an error if the bundled model template is invalid.
pub fn render_rust_models(
    models: &[RustModelTemplate],
    aliases: &[RustTypeAliasTemplate],
) -> anyhow::Result<String> {
    let data = ModelSourceTemplate {
        has_optional_fields: models
            .iter()
            .filter(|model| !model.sequence)
            .flat_map(|model| &model.fields)
            .any(|field| field.optional),
        has_required_fields: models
            .iter()
            .filter(|model| !model.sequence)
            .flat_map(|model| &model.fields)
            .any(|field| field.required),
        models,
        aliases,
    };
    Ok(ramhorns::Template::new(include_str!("models.rs.template"))?.render(&data))
}

fn date_shape(schema: &JsonValue) -> bool {
    if schema.get("x-forklaunch-date-codec").is_some() {
        return true;
    }
    if let Some(inner) = non_null_schema(schema) {
        return date_shape(&inner);
    }
    schema.get("type").and_then(JsonValue::as_str) == Some("array")
        && schema.get("items").is_some_and(date_shape)
}

#[cfg(test)]
mod sequence_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn generated_model_names_cannot_collide_with_sequence_internals() {
        for name in [
            "SequenceVisitor",
            "A",
            "D",
            "S",
            "__ForklaunchSequenceVisitor",
            "__ForklaunchSerializer",
            "__ForklaunchDeserializer",
            "__ForklaunchSeqAccess",
            "",
            "12 _ A",
            "Self",
            "Self_",
        ] {
            let generated = rust_type_name(name);
            assert!(
                generated
                    .trim_end_matches('_')
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric())
            );
            assert!(!generated.starts_with('_'));
        }
    }

    #[test]
    fn reserved_type_keyword_normalization_is_idempotent() {
        for name in ["Self", "self", "SELF", "Self_"] {
            let normalized = rust_type_name(name);
            assert_eq!(normalized, "Self_");
            assert_eq!(rust_type_name(&normalized), normalized);
        }
        for name in ["ModelSelf", "Selfish", "A", "SequenceVisitor"] {
            assert_eq!(rust_type_name(name), name);
        }
    }

    #[test]
    fn positional_shapes_preserve_presence_rest_and_bounds() {
        for count in [0, 1, 16, 17, 128] {
            let schema = json!({"type":"array", "prefixItems":vec![json!({"type":"string"});count], "items":false, "minItems":count});
            let (models, _, name) = rust_types_from_schema("Tuple", &schema);
            let model = models.iter().find(|model| model.name == name).unwrap();
            assert!(model.sequence);
            assert_eq!(model.fields.len(), count);
            assert!(model.has_default);
            assert!(!model.sequence_rest);
        }
        let schema = json!({"type":"array","prefixItems":[{"type":["string","null"]},{"type":"boolean"}],"items":{"type":"integer"},"maxItems":4});
        let (models, _, _) = rust_types_from_schema("Tuple", &schema);
        assert_eq!(
            models[0].fields[0].field_type,
            "::std::option::Option<::std::option::Option<::std::string::String>>"
        );
        assert_eq!(
            models[0].fields[1].field_type,
            "::std::option::Option<bool>"
        );
        assert_eq!(models[0].sequence_rest_type, "i64");
        assert!(models[0].has_default);
        let required_rest = json!({"type":"array","prefixItems":[true],"minItems":3,"items":true});
        let (models, _, _) = rust_types_from_schema("Tuple", &required_rest);
        assert!(!models[0].has_default);
        assert_eq!(models[0].sequence_rest_type, "::serde_json::Value");
        assert!(
            sequence_shape(
                &json!({"type":"array","prefixItems":[true],"items":false,"minItems":2})
            )
            .is_none()
        );
        assert!(sequence_shape(&json!({"type":"array","items":{"type":"string"}})).is_none());
        assert_eq!(
            sequence_shape(&json!({"type":"array","maxItems":0}))
                .unwrap()
                .prefix
                .len(),
            0
        );
        assert_eq!(
            sequence_shape(
                &json!({"type":"array","prefixItems":[true,true],"minItems":1,"maxItems":1})
            )
            .unwrap()
            .prefix
            .len(),
            1
        );
    }

    #[test]
    fn implicit_draft_seven_ignores_modern_positions() {
        let schema = json!({"type":"array","prefixItems":[{"type":"number"}],"items":{"type":"string"},"minItems":1,"maxItems":1});
        let normalized = super::super::schema::normalize(&schema, "Legacy").unwrap();
        assert!(normalized.get("prefixItems").is_none());
        let (models, aliases, ty) = rust_types_from_schema("Legacy", &normalized);
        assert!(models.is_empty());
        assert_eq!(ty, "Legacy");
        assert_eq!(
            aliases[0].target_type,
            "::std::vec::Vec<::std::string::String>"
        );
    }

    #[test]
    fn required_recursive_position_does_not_derive_default() {
        let schema = json!({"$schema":"https://json-schema.org/draft/2020-12/schema", "$defs":{"Node":{"type":"array","prefixItems":[{"$ref":"#/$defs/Node"}],"minItems":1,"items":false}}, "$ref":"#/$defs/Node"});
        let schema = super::super::schema::normalize(&schema, "Recursive").unwrap();
        let (models, aliases, _) = rust_types_from_schema("Recursive", &schema);
        let sequence = models.iter().find(|model| model.sequence).unwrap();
        assert!(!sequence.has_default);
        assert!(
            sequence.fields[0]
                .field_type
                .starts_with("::std::boxed::Box<")
        );
        let source = render_rust_models(&models, &aliases).unwrap();
        assert!(source.contains("deserialize_seq"));
        assert!(source.contains("extra tuple element"));
        assert!(!source.contains("PartialEq, Serialize, Deserialize)]\npub struct"));
    }
}

#[cfg(test)]
mod string_enum_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn closed_string_sets_become_serde_renamed_enums() {
        let schema = json!({"type":"object","required":["currency","methods"],"properties":{
            "currency":{"type":"string","enum":["USD","eur","usd","9x","self"]},
            "methods":{"type":"array","items":{"enum":["card","klarna"]}},
            "open":{"type":"string"},
            "mixed":{"enum":["a",1]}
        }});
        let (models, aliases, root) = rust_types_from_schema("Payment", &schema);
        let source = render_rust_models(&models, &aliases).unwrap();
        assert_eq!(root, "Payment");
        assert!(source.contains("pub enum PaymentCurrency {"), "{source}");
        for (variant, wire) in [
            ("Usd", "USD"),
            ("Eur", "eur"),
            ("Usd2", "usd"),
            ("Model9X", "9x"),
            ("Value5", "self"),
        ] {
            assert!(
                source.contains(&format!("#[serde(rename = \"{wire}\")]\n    {variant},")),
                "{variant}"
            );
        }
        assert!(source.contains("pub enum PaymentMethodsItem {"));
        assert!(source.contains("::std::vec::Vec<PaymentMethodsItem>"));
        assert!(source.contains("pub open: ::std::option::Option<::std::string::String>"));
        assert!(!source.contains("pub enum PaymentMixed"));
        assert!(source.contains("Self::Usd => \"USD\""));
    }
}
