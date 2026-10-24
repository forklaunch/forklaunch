//! Typed response headers are schema-bearing values, rather than ad hoc mutations.
use super::{Endpoint, EndpointBody};
use std::{collections::BTreeMap, marker::PhantomData};

pub struct WithResponseHeaders<O, H>(PhantomData<fn() -> (O, H)>);
pub struct ResponseWithHeaders<B, H> {
    pub body: B,
    pub headers: H,
}
impl<O: EndpointBody, H: schemars::JsonSchema + Send + 'static> EndpointBody
    for WithResponseHeaders<O, H>
{
    type Value = ResponseWithHeaders<O::Value, H>;
    fn contract() -> Option<crate::contract::BodyContract> {
        O::contract()
    }
    fn response_enum() -> bool {
        O::response_enum()
    }
    fn response_contracts(status: u16) -> Vec<crate::contract::ResponseContract> {
        O::response_contracts(status)
    }
}
impl<I: EndpointBody, O: EndpointBody> Endpoint<I, O> {
    /// Declare a typed object of response headers on every application variant.
    /// Optional fields may be omitted; transport-managed headers are prohibited.
    /// # Errors
    /// Rejects unsupported schemas, duplicate names and transport-managed headers.
    pub fn with_typed_response_headers<H: schemars::JsonSchema + Send + 'static>(
        mut self,
    ) -> Result<Endpoint<I, WithResponseHeaders<O, H>>, &'static str> {
        let headers = super::fields::fields::<H>(true, false)?;
        for field in &headers {
            if matches!(
                field.name.to_ascii_lowercase().as_str(),
                "content-type"
                    | "content-length"
                    | "transfer-encoding"
                    | "connection"
                    | "set-cookie"
            ) {
                return Err("response header is managed by the transport");
            }
        }
        for index in &self.output_indices {
            let response = &mut self.route.responses[*index];
            for header in &headers {
                if response
                    .headers
                    .iter()
                    .any(|existing| existing.name.eq_ignore_ascii_case(&header.name))
                {
                    return Err("response header is already declared");
                }
                let mut header = header.clone();
                if super::FRAMEWORK_FAILURE_STATUSES.contains(&response.status_code)
                    && super::response_identity(response).1 == "application/json"
                {
                    header.required = false;
                }
                response.headers.push(header);
            }
        }
        Ok(Endpoint {
            route: self.route,
            output_indices: self.output_indices,
            marker: PhantomData,
        })
    }
}
/// Serialize header values using the same scalar/JSON representation as SDK metadata.
/// # Errors
/// Rejects non-object values or invalid HTTP names/values at runtime.
pub fn encode_response_headers<H: serde::Serialize>(
    headers: H,
) -> Result<BTreeMap<String, String>, super::ResponseEncodingError> {
    let value = serde_json::to_value(headers).map_err(|_| super::ResponseEncodingError)?;
    let object = value.as_object().ok_or(super::ResponseEncodingError)?;
    object
        .iter()
        .filter(|(_, value)| !value.is_null())
        .map(|(name, value)| {
            let wire = value
                .as_str()
                .map_or_else(|| value.to_string(), str::to_owned);
            if !wire
                .bytes()
                .all(|byte| byte == b'\t' || (32..127).contains(&byte))
            {
                return Err(super::ResponseEncodingError);
            }
            Ok((name.to_ascii_lowercase(), wire))
        })
        .collect()
}
