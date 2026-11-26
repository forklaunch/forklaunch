//! Typed request metadata projected into the same per-field schemas used by SDKs.
use super::EndpointBody;
use crate::contract::{BodyContract, FieldContract, SchemaRef};
use std::marker::PhantomData;

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct NoFields {}

type FieldMarker<I, Q, H, P> = PhantomData<fn() -> (I, Q, H, P)>;
pub struct Fields<I, Q = NoFields, H = NoFields, P = NoFields>(FieldMarker<I, Q, H, P>);
pub struct RequestFields<B, Q, H, P = NoFields> {
    pub path: P,
    pub body: B,
    pub query: Q,
    pub headers: H,
}
impl<I, Q, H, P> EndpointBody for Fields<I, Q, H, P>
where
    I: EndpointBody,
    Q: schemars::JsonSchema + Send + 'static,
    H: schemars::JsonSchema + Send + 'static,
    P: schemars::JsonSchema + Send + 'static,
{
    type Value = RequestFields<I::Value, Q, H, P>;
    fn contract() -> Option<BodyContract> {
        I::contract()
    }
    fn body_optional() -> bool {
        I::body_optional()
    }
    fn query_fields() -> Result<Vec<FieldContract>, &'static str> {
        projected_fields::<Q>(false, true, true)
    }
    fn header_fields() -> Result<Vec<FieldContract>, &'static str> {
        projected_fields::<H>(true, true, true)
    }
    fn query_schema() -> Result<Option<SchemaRef>, &'static str> {
        object_schema::<Q>()
    }
    fn headers_schema() -> Result<Option<SchemaRef>, &'static str> {
        object_schema::<H>()
    }
    fn path_fields() -> Result<Option<Vec<FieldContract>>, &'static str> {
        let fields = fields::<P>(false, true)?;
        if fields.iter().any(|field| !field.required) {
            return Err("path fields must be required");
        }
        Ok((!fields.is_empty()).then_some(fields))
    }
    fn request_only() -> bool {
        true
    }
}

pub(super) fn fields<T: schemars::JsonSchema>(
    headers: bool,
    input: bool,
) -> Result<Vec<FieldContract>, &'static str> {
    projected_fields::<T>(headers, input, false)
}
fn projected_fields<T: schemars::JsonSchema>(
    headers: bool,
    input: bool,
    whole: bool,
) -> Result<Vec<FieldContract>, &'static str> {
    let root = crate::contract::schema_value_for::<T>();
    let projected = if input {
        crate::contract::codec::input_schema(&root)
    } else {
        root.clone()
    };
    let directional = projected != root;
    let root = projected;
    let object = root
        .as_object()
        .ok_or("request fields require an object schema")?;
    if root.get("type").and_then(serde_json::Value::as_str) != Some("object")
        || object.keys().any(|key| {
            !((whole && object_constraint(key))
                || matches!(
                    key.as_str(),
                    "$schema"
                        | "$defs"
                        | "definitions"
                        | "title"
                        | "description"
                        | "x-forklaunch-model"
                        | "type"
                        | "properties"
                        | "required"
                        | "additionalProperties"
                ))
        })
        || root
            .get("additionalProperties")
            .is_some_and(|value| value != &serde_json::Value::Bool(false))
    {
        return Err("request fields require a fixed object schema with supported constraints");
    }
    let Some(properties) = root
        .get("properties")
        .and_then(serde_json::Value::as_object)
    else {
        return Ok(Vec::new());
    };
    let required = root.get("required").and_then(serde_json::Value::as_array);
    let mut names = std::collections::BTreeSet::new();
    properties
        .iter()
        .map(|(name, schema)| {
            if headers
                && (name.is_empty()
                    || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
                    || !names.insert(name.to_ascii_lowercase()))
            {
                return Err("header fields require unique case-insensitive HTTP names");
            }
            let mut schema = schema.clone();
            for key in ["$defs", "definitions"] {
                if let Some(definitions) = root.get(key) {
                    schema
                        .as_object_mut()
                        .ok_or("field schema must be an object")?
                        .insert(key.into(), definitions.clone());
                }
            }
            Ok(FieldContract {
                name: name.clone(),
                required: required
                    .is_some_and(|fields| fields.iter().any(|field| field.as_str() == Some(name))),
                schema: SchemaRef::named(format!(
                    "{}{}{}",
                    T::schema_name(),
                    name,
                    if directional { "WireInput" } else { "" }
                ))
                .with_json_schema(schema.to_string()),
            })
        })
        .collect()
}

fn object_constraint(key: &str) -> bool {
    matches!(
        key,
        "minProperties"
            | "maxProperties"
            | "dependencies"
            | "dependentRequired"
            | "dependentSchemas"
            | "allOf"
            | "anyOf"
            | "oneOf"
            | "not"
            | "if"
            | "then"
            | "else"
            | "unevaluatedProperties"
    )
}
fn object_schema<T: schemars::JsonSchema>() -> Result<Option<SchemaRef>, &'static str> {
    let root = crate::contract::codec::input_schema(&crate::contract::schema_value_for::<T>());
    let object = root
        .as_object()
        .ok_or("request fields require an object schema")?;
    Ok(object.keys().any(|key| object_constraint(key)).then(|| {
        SchemaRef::named(format!("{}WireInput", T::schema_name()))
            .with_json_schema(root.to_string())
    }))
}
