use super::{EndpointError, NativeBody, request_schema_error};
use axum::http::request::Parts;
use forklaunch_core::contract::{
    FieldContract,
    endpoint::{Fields, RequestFields},
    schema_validation::{CheckedJsonValidator, compile_checked},
};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use std::{collections::BTreeMap, sync::Arc};

impl<I, Q, H, P> NativeBody for Fields<I, Q, H, P>
where
    I: NativeBody,
    Q: schemars::JsonSchema + DeserializeOwned + Send + 'static,
    H: schemars::JsonSchema + DeserializeOwned + Send + 'static,
    P: schemars::JsonSchema + DeserializeOwned + Send + 'static,
{
    #[allow(clippy::unused_async_trait_impl)]
    async fn decode(
        _: &[u8],
        _: &str,
        _: Option<&CheckedJsonValidator>,
    ) -> Result<Self::Value, EndpointError> {
        Err(EndpointError::InvalidRequest)
    }
    async fn decode_request(
        bytes: &[u8],
        content_type: &str,
        validator: Option<&CheckedJsonValidator>,
        query: Value,
        headers: Value,
        path: Value,
    ) -> Result<Self::Value, EndpointError> {
        Ok(RequestFields {
            path: serde_json::from_value(path).map_err(|_| EndpointError::InvalidRequest)?,
            body: I::decode(bytes, content_type, validator).await?,
            query: serde_json::from_value(query).map_err(|_| EndpointError::InvalidRequest)?,
            headers: serde_json::from_value(headers).map_err(|_| EndpointError::InvalidRequest)?,
        })
    }
    async fn decode_profile_request(
        bytes: &[u8],
        content_type: &str,
        validator: Option<&CheckedJsonValidator>,
        parsed: Option<Value>,
        metadata: (Value, Value, Value),
    ) -> Result<Self::Value, EndpointError> {
        Ok(RequestFields {
            path: serde_json::from_value(metadata.2).map_err(|_| EndpointError::InvalidRequest)?,
            body: I::decode_profile_request(
                bytes,
                content_type,
                validator,
                parsed,
                (Value::Null, Value::Null, Value::Null),
            )
            .await?,
            query: serde_json::from_value(metadata.0).map_err(|_| EndpointError::InvalidRequest)?,
            headers: serde_json::from_value(metadata.1)
                .map_err(|_| EndpointError::InvalidRequest)?,
        })
    }
    fn encode(_: Self::Value, _: Option<&CheckedJsonValidator>) -> Result<Vec<u8>, EndpointError> {
        Err(EndpointError::InvalidResponse)
    }
}

struct Field {
    name: String,
    required: bool,
    validate: Arc<CheckedJsonValidator>,
}
pub(super) struct FieldPlans {
    path: Vec<Field>,
    query: Vec<Field>,
    headers: Vec<Field>,
    query_object: Option<Arc<CheckedJsonValidator>>,
    headers_object: Option<Arc<CheckedJsonValidator>>,
}
impl FieldPlans {
    pub fn new(
        path: &[FieldContract],
        query: &[FieldContract],
        headers: &[FieldContract],
        query_schema: Option<&forklaunch_core::contract::SchemaRef>,
        headers_schema: Option<&forklaunch_core::contract::SchemaRef>,
    ) -> Result<Self, String> {
        Ok(Self {
            path: compile(path)?,
            query: compile(query)?,
            headers: compile(headers)?,
            query_object: compile_object(query_schema)?,
            headers_object: compile_object(headers_schema)?,
        })
    }
    pub fn has_path(&self) -> bool {
        !self.path.is_empty()
    }
    pub(super) fn raw_metadata(
        &self,
        parts: &Parts,
        path: &std::collections::HashMap<String, String>,
    ) -> Result<(Value, Value, Value), EndpointError> {
        fn value(values: Vec<String>) -> Value {
            if values.len() == 1 {
                Value::String(values[0].clone())
            } else {
                Value::Array(values.into_iter().map(Value::String).collect())
            }
        }
        let pairs: Vec<(String, String)> =
            serde_urlencoded::from_str(parts.uri.query().unwrap_or(""))
                .map_err(|_| EndpointError::InvalidRequest)?;
        let mut raw = BTreeMap::<String, Vec<String>>::new();
        for (name, item) in pairs {
            raw.entry(name).or_default().push(item);
        }
        let query = raw
            .into_iter()
            .map(|(name, items)| (name, value(items)))
            .collect();
        let mut headers = Map::new();
        for field in &self.headers {
            let values = parts
                .headers
                .get_all(&field.name)
                .iter()
                .map(|v| {
                    v.to_str()
                        .map(str::to_owned)
                        .map_err(|_| EndpointError::InvalidRequest)
                })
                .collect::<Result<Vec<_>, _>>()?;
            if !values.is_empty() {
                headers.insert(field.name.clone(), value(values));
            }
        }
        Ok((
            Value::Object(query),
            Value::Object(headers),
            Value::Object(
                path.iter()
                    .map(|(name, item)| (name.clone(), Value::String(item.clone())))
                    .collect(),
            ),
        ))
    }
    pub fn decode_path(
        &self,
        values: &std::collections::HashMap<String, String>,
    ) -> Result<Value, EndpointError> {
        let mut object = Map::new();
        for field in &self.path {
            let value = values
                .get(&field.name)
                .ok_or(EndpointError::InvalidRequest)?;
            let value = field
                .decode(std::slice::from_ref(value))?
                .ok_or(EndpointError::InvalidRequest)?;
            object.insert(field.name.clone(), value);
        }
        Ok(Value::Object(object))
    }
    pub fn decode(&self, parts: &Parts) -> Result<(Value, Value), EndpointError> {
        let pairs: Vec<(String, String)> =
            serde_urlencoded::from_str(parts.uri.query().unwrap_or(""))
                .map_err(|_| EndpointError::InvalidRequest)?;
        let mut raw = BTreeMap::<String, Vec<String>>::new();
        for (name, value) in pairs {
            raw.entry(name).or_default().push(value);
        }
        let mut query = Map::new();
        for field in &self.query {
            if let Some(value) = field.decode(&raw.remove(&field.name).unwrap_or_default())? {
                query.insert(field.name.clone(), value);
            }
        }
        // Preserve undeclared query fields so serde(deny_unknown_fields) remains effective.
        for (name, values) in raw {
            let value = if values.len() == 1 {
                Value::String(values[0].clone())
            } else {
                Value::Array(values.into_iter().map(Value::String).collect())
            };
            query.insert(name, value);
        }
        let mut headers = Map::new();
        for field in &self.headers {
            let values = parts
                .headers
                .get_all(&field.name)
                .iter()
                .map(|value| {
                    value
                        .to_str()
                        .map(str::to_owned)
                        .map_err(|_| EndpointError::InvalidRequest)
                })
                .collect::<Result<Vec<_>, _>>()?;
            if let Some(value) = field.decode(&values)? {
                headers.insert(field.name.clone(), value);
            }
        }
        let query = Value::Object(query);
        let headers = Value::Object(headers);
        for (plan, value) in [
            (&self.query_object, &query),
            (&self.headers_object, &headers),
        ] {
            if let Some(plan) = plan {
                plan(value).map_err(|error| request_schema_error(&error))?;
            }
        }
        Ok((query, headers))
    }
}
fn compile_object(
    schema: Option<&forklaunch_core::contract::SchemaRef>,
) -> Result<Option<Arc<CheckedJsonValidator>>, String> {
    schema
        .map(|schema| {
            let value = serde_json::from_str(
                schema
                    .json_schema
                    .as_deref()
                    .ok_or("missing request object schema")?,
            )
            .map_err(|_| "invalid request object schema")?;
            compile_checked(&value)
        })
        .transpose()
}
fn compile(fields: &[FieldContract]) -> Result<Vec<Field>, String> {
    fields
        .iter()
        .map(|field| {
            let schema = field
                .schema
                .json_schema
                .as_ref()
                .ok_or("missing request field schema")?;
            let schema =
                serde_json::from_str(schema).map_err(|_| "invalid request field schema")?;
            Ok(Field {
                name: field.name.clone(),
                required: field.required,
                validate: compile_checked(&schema).map_err(|_| "invalid request field schema")?,
            })
        })
        .collect()
}
impl Field {
    fn decode(&self, values: &[String]) -> Result<Option<Value>, EndpointError> {
        forklaunch_core::contract::schema_validation::decode_wire_field(
            self.validate.as_ref(),
            values,
            self.required,
        )
        .map_err(|error| request_schema_error(&error))
    }
}

pub(super) fn version_hook(
    field: &FieldContract,
    raw_header: bool,
) -> Result<forklaunch_core::http::VersionFieldHooks, String> {
    let compiled = compile(std::slice::from_ref(field))?.remove(0);
    Ok(
        forklaunch_core::http::VersionFieldHooks::default().with_checked_field_preprocess(move |raw| {
            let values = raw
                .map(|value| {
                    value
                        .as_str()
                        .ok_or_else(|| forklaunch_core::contract::schema_validation::SchemaValidationFailure::Invalid("invalid native field projection".into()))
                        .and_then(|value| {
                            if raw_header {
                                Ok(vec![value.to_owned()])
                            } else {
                                serde_json::from_str::<Vec<String>>(value)
                                    .map_err(|_| forklaunch_core::contract::schema_validation::SchemaValidationFailure::Invalid("invalid native field projection".into()))
                            }
                        })
                })
                .transpose()?
                .unwrap_or_default();
            forklaunch_core::contract::schema_validation::decode_wire_field(
                compiled.validate.as_ref(), &values, compiled.required,
            )
        }),
    )
}
