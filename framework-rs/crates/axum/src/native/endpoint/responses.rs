use super::{EndpointError, NativeBody, validator};
use axum::{
    body::Body,
    http::{HeaderName, HeaderValue, StatusCode},
    response::Response,
};
use forklaunch_core::contract::{
    BodyContract, BodyKind, ResponseContract, RouteContract, ValidationMode,
    endpoint::{EncodedResponse, ResponseVariants, Responses},
    schema_validation::{CheckedJsonValidator, compile_checked},
};
use std::{collections::BTreeMap, sync::Arc};

impl<E: ResponseVariants> NativeBody for Responses<E> {
    #[allow(clippy::unused_async_trait_impl)]
    async fn decode(
        _: &[u8],
        _: &str,
        _: Option<&CheckedJsonValidator>,
    ) -> Result<E, EndpointError> {
        Err(EndpointError::InvalidRequest)
    }
    fn encode(_: E, _: Option<&CheckedJsonValidator>) -> Result<Vec<u8>, EndpointError> {
        Err(EndpointError::InvalidResponse)
    }
    fn encode_response(
        value: E,
        _: u16,
        _: Option<&CheckedJsonValidator>,
    ) -> Result<EncodedResponse, EndpointError> {
        value.encode().map_err(|_| EndpointError::InvalidResponse)
    }
}

#[derive(Clone)]
struct AssertionPolicy {
    mode: ValidationMode,
    operation: String,
}
impl AssertionPolicy {
    fn check(
        &self,
        location: &'static str,
        assertion: impl FnOnce() -> Result<bool, EndpointError>,
    ) -> Result<(), EndpointError> {
        if self.mode == ValidationMode::None {
            return Ok(());
        }
        if assertion()? {
            return Ok(());
        }
        if self.mode == ValidationMode::Warning {
            tracing::warn!(operation = %self.operation, reason = "schema_rejection", location, "response schema observation failed; original response retained");
            Ok(())
        } else {
            Err(EndpointError::InvalidResponse)
        }
    }
}
pub(super) fn configure(
    mut plans: BTreeMap<(u16, String), ResponsePlan>,
    route: &RouteContract,
) -> BTreeMap<(u16, String), ResponsePlan> {
    for plan in plans.values_mut() {
        plan.assertions = AssertionPolicy {
            mode: route.options.response_validation,
            operation: route.operation_name.clone(),
        };
    }
    plans
}
pub(super) struct ResponsePlan {
    assertions: AssertionPolicy,
    status: StatusCode,
    body: Option<BodyContract>,
    pub validator: Option<Arc<CheckedJsonValidator>>,
    headers: Vec<(HeaderName, HeaderValue)>,
    dynamic_headers: Vec<(HeaderName, bool, Arc<CheckedJsonValidator>)>,
}
impl ResponsePlan {
    pub fn render(
        &self,
        bytes: Vec<u8>,
        mut values: BTreeMap<String, String>,
    ) -> Result<Response, EndpointError> {
        let known = values.keys().all(|name| {
            self.headers.iter().any(|(key, _)| key.as_str() == name)
                || self
                    .dynamic_headers
                    .iter()
                    .any(|(key, _, _)| key.as_str() == name)
        });
        let mut headers: BTreeMap<String, String> = self
            .headers
            .iter()
            .map(|(name, value)| {
                value
                    .to_str()
                    .map(|value| (name.to_string(), value.to_owned()))
                    .map_err(|_| EndpointError::InvalidResponse)
            })
            .collect::<Result<_, _>>()?;
        if values
            .get("content-type")
            .is_some_and(|value| headers.get("content-type") != Some(value))
        {
            return Err(EndpointError::InvalidResponse);
        }
        headers.append(&mut values);
        self.assertions
            .check("headers", || Ok(known && self.accepts_headers(&headers)?))?;
        self.validate_data(&bytes)?;

        let mut response = Response::new(Body::from(bytes));
        *response.status_mut() = self.status;
        for (name, value) in headers {
            response.headers_mut().insert(
                name.parse::<HeaderName>()
                    .map_err(|_| EndpointError::InvalidResponse)?,
                value.parse().map_err(|_| EndpointError::InvalidResponse)?,
            );
        }
        Ok(response)
    }
}
pub(super) fn version_plans(
    responses: &[ResponseContract],
) -> Result<BTreeMap<(u16, String), ResponsePlan>, String> {
    let responses = responses
        .iter()
        .cloned()
        .map(|mut response| {
            if let Some(body) = &mut response.body
                && body.kind == BodyKind::ServerSentEvent
            {
                body.kind = BodyKind::Json;
            }
            response
        })
        .collect::<Vec<_>>();
    plans(&responses)
}
pub(super) fn plans(
    responses: &[ResponseContract],
) -> Result<BTreeMap<(u16, String), ResponsePlan>, String> {
    responses
        .iter()
        .map(|response| {
            if response.body.as_ref().is_some_and(|body| {
                matches!(body.kind, BodyKind::Multipart | BodyKind::ServerSentEvent)
            }) {
                return Err("response body requires a dedicated streaming adapter".into());
            }
            let mut headers = Vec::new();
            let mut dynamic_headers = Vec::new();
            for header in &response.headers {
                let name: HeaderName =
                    header.name.parse().map_err(|_| "invalid response header")?;
                let schema: serde_json::Value = serde_json::from_str(
                    header
                        .schema
                        .json_schema
                        .as_deref()
                        .ok_or("missing response header schema")?,
                )
                .map_err(|_| "invalid response header schema")?;
                if let Some(value) = schema.get("const").and_then(serde_json::Value::as_str) {
                    headers.push((name, value.parse().map_err(|_| "invalid response header")?));
                } else {
                    dynamic_headers.push((
                        name,
                        header.required,
                        compile_checked(&schema).map_err(|_| "invalid response header schema")?,
                    ));
                }
            }
            if let Some(content_type) = response
                .body
                .as_ref()
                .and_then(|body| body.content_type.as_ref())
            {
                headers.push((
                    axum::http::header::CONTENT_TYPE,
                    content_type.parse().map_err(|_| "invalid content type")?,
                ));
            }
            Ok((
                forklaunch_core::contract::endpoint::response_identity(response),
                ResponsePlan {
                    assertions: AssertionPolicy {
                        mode: ValidationMode::Error,
                        operation: String::new(),
                    },
                    status: StatusCode::from_u16(response.status_code)
                        .map_err(|_| "invalid response status")?,
                    validator: validator(response.body.as_ref())?,
                    body: response.body.clone(),
                    headers,
                    dynamic_headers,
                },
            ))
        })
        .collect()
}

impl<O, H> NativeBody for forklaunch_core::contract::endpoint::WithResponseHeaders<O, H>
where
    O: NativeBody,
    H: forklaunch_core::contract::__private::schemars::JsonSchema
        + serde::Serialize
        + Send
        + 'static,
{
    #[allow(clippy::unused_async_trait_impl)]
    async fn decode(
        _: &[u8],
        _: &str,
        _: Option<&CheckedJsonValidator>,
    ) -> Result<Self::Value, EndpointError> {
        Err(EndpointError::InvalidRequest)
    }
    fn encode(_: Self::Value, _: Option<&CheckedJsonValidator>) -> Result<Vec<u8>, EndpointError> {
        Err(EndpointError::InvalidResponse)
    }
    fn encode_response(
        value: Self::Value,
        status: u16,
        validator: Option<&CheckedJsonValidator>,
    ) -> Result<EncodedResponse, EndpointError> {
        let mut encoded = O::encode_response(value.body, status, validator)?;
        encoded.headers =
            forklaunch_core::contract::endpoint::encode_response_headers(value.headers)
                .map_err(|_| EndpointError::InvalidResponse)?;
        Ok(encoded)
    }
}

/// Trusted request-selection provenance, never populated from caller headers.
#[derive(Clone)]
pub(super) struct MatchedResponsePlans(pub Vec<Arc<BTreeMap<(u16, String), ResponsePlan>>>);
impl MatchedResponsePlans {
    pub fn render(
        &self,
        selected: &ResponsePlan,
        identity: &(u16, String),
        bytes: Vec<u8>,
        values: BTreeMap<String, String>,
    ) -> Result<Response, EndpointError> {
        self.render_inner(selected, identity, bytes, values, true)
    }
    #[cfg(feature = "native-streams")]
    pub(super) fn render_stream_headers(
        &self,
        selected: &ResponsePlan,
        identity: &(u16, String),
        values: BTreeMap<String, String>,
    ) -> Result<Response, EndpointError> {
        self.render_inner(selected, identity, Vec::new(), values, false)
    }
    #[cfg(feature = "native-streams")]
    pub(super) fn validate_stream_data(
        &self,
        identity: &(u16, String),
        bytes: &[u8],
    ) -> Result<(), EndpointError> {
        let selected = self
            .0
            .iter()
            .rev()
            .find_map(|plans| plans.get(identity))
            .ok_or(EndpointError::InvalidResponse)?;
        selected.validate_wire(bytes)?;
        selected.assertions.check("body", || {
            any_match(
                self.0
                    .iter()
                    .filter_map(|plans| plans.get(identity))
                    .map(|plan| plan.accepts_body(bytes)),
            )
        })
    }
    fn render_inner(
        &self,
        selected: &ResponsePlan,
        identity: &(u16, String),
        bytes: Vec<u8>,
        values: BTreeMap<String, String>,
        check_body: bool,
    ) -> Result<Response, EndpointError> {
        let candidates: Vec<_> = self
            .0
            .iter()
            .filter_map(|plans| plans.get(identity))
            .collect();
        if candidates.is_empty() {
            return Err(EndpointError::InvalidResponse);
        }
        // Apply the selected handler's defaults, but independently admit headers
        // and bodies from any matched branch, as TypeScript response middleware does.
        let mut headers: BTreeMap<String, String> = selected
            .headers
            .iter()
            .map(|(name, value)| {
                value
                    .to_str()
                    .map(|value| (name.to_string(), value.to_owned()))
                    .map_err(|_| EndpointError::InvalidResponse)
            })
            .collect::<Result<_, _>>()?;
        let unknown = values.keys().any(|name| {
            !candidates.iter().any(|plan| {
                plan.headers.iter().any(|(known, _)| known.as_str() == name)
                    || plan
                        .dynamic_headers
                        .iter()
                        .any(|(known, _, _)| known.as_str() == name)
            })
        });
        if values
            .get("content-type")
            .is_some_and(|value| headers.get("content-type") != Some(value))
        {
            return Err(EndpointError::InvalidResponse);
        }
        headers.extend(values);
        if check_body {
            selected.validate_wire(&bytes)?;
            selected.assertions.check("body", || {
                any_match(candidates.iter().map(|plan| plan.accepts_body(&bytes)))
            })?;
        }
        selected.assertions.check("headers", || {
            Ok(
                !unknown
                    && any_match(candidates.iter().map(|plan| plan.accepts_headers(&headers)))?,
            )
        })?;
        let mut response = Response::new(Body::from(bytes));
        *response.status_mut() = selected.status;
        for (name, value) in headers {
            response.headers_mut().insert(
                name.parse::<HeaderName>()
                    .map_err(|_| EndpointError::InvalidResponse)?,
                value.parse().map_err(|_| EndpointError::InvalidResponse)?,
            );
        }
        Ok(response)
    }
}
fn any_match(
    candidates: impl Iterator<Item = Result<bool, EndpointError>>,
) -> Result<bool, EndpointError> {
    for candidate in candidates {
        if candidate? {
            return Ok(true);
        }
    }
    Ok(false)
}
fn schema_matches(
    validate: Option<&CheckedJsonValidator>,
    value: &serde_json::Value,
) -> Result<bool, EndpointError> {
    use forklaunch_core::contract::schema_validation::SchemaValidationFailure;
    match validate.map(|validate| validate(value)) {
        None | Some(Ok(())) => Ok(true),
        Some(Err(SchemaValidationFailure::Invalid(_))) => Ok(false),
        Some(Err(SchemaValidationFailure::Operational)) => Err(EndpointError::InvalidResponse),
    }
}
impl ResponsePlan {
    fn validate_wire(&self, bytes: &[u8]) -> Result<(), EndpointError> {
        match self.body.as_ref().map(|body| body.kind) {
            None if !bytes.is_empty() => Err(EndpointError::InvalidResponse),
            Some(BodyKind::Json) => serde_json::from_slice::<serde_json::Value>(bytes)
                .map(|_| ())
                .map_err(|_| EndpointError::InvalidResponse),
            Some(BodyKind::Text) => std::str::from_utf8(bytes)
                .map(|_| ())
                .map_err(|_| EndpointError::InvalidResponse),
            _ => Ok(()),
        }
    }
    pub(super) fn validate_data(&self, bytes: &[u8]) -> Result<(), EndpointError> {
        self.validate_wire(bytes)?;
        self.assertions.check("body", || self.accepts_body(bytes))
    }
    fn accepts_body(&self, bytes: &[u8]) -> Result<bool, EndpointError> {
        let value = match &self.body {
            None => return Ok(bytes.is_empty()),
            Some(body) if body.kind == BodyKind::Json => match serde_json::from_slice(bytes) {
                Ok(value) => value,
                Err(_) => return Ok(false),
            },
            Some(body) if body.kind == BodyKind::Text => match std::str::from_utf8(bytes) {
                Ok(value) => serde_json::Value::String(value.into()),
                Err(_) => return Ok(false),
            },
            Some(_) => return Ok(true),
        };
        schema_matches(self.validator.as_deref(), &value)
    }
    fn accepts_headers(&self, values: &BTreeMap<String, String>) -> Result<bool, EndpointError> {
        if !self.headers.iter().all(|(name, fixed)| {
            values
                .get(name.as_str())
                .is_some_and(|value| value.as_bytes() == fixed.as_bytes())
        }) {
            return Ok(false);
        }
        for (name, required, validate) in &self.dynamic_headers {
            let Some(value) = values.get(name.as_str()) else {
                if *required {
                    return Ok(false);
                }
                continue;
            };
            if !schema_matches(
                Some(validate.as_ref()),
                &serde_json::Value::String(value.clone()),
            )? {
                let Ok(value) = serde_json::from_str(value) else {
                    return Ok(false);
                };
                if !schema_matches(Some(validate.as_ref()), &value)? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}
