//! Host-compiled response variants bind a status and media type to each Rust value.
use super::{Endpoint, EndpointBody};
use crate::contract::{BodyContract, HttpMethod, ResponseContract, RouteContract};
use std::marker::PhantomData;

pub struct EncodedResponse {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
    pub headers: std::collections::BTreeMap<String, String>,
}
#[derive(Clone, Copy, Debug)]
pub struct ResponseEncodingError;

impl EncodedResponse {
    /// # Errors
    /// Rejects a response value that cannot be serialized as JSON.
    pub fn json<T: serde::Serialize>(
        status: u16,
        value: &T,
    ) -> Result<Self, ResponseEncodingError> {
        Ok(Self {
            status,
            headers: std::collections::BTreeMap::new(),
            content_type: "application/json".into(),
            body: serde_json::to_vec(value).map_err(|_| ResponseEncodingError)?,
        })
    }
    #[must_use]
    pub fn text(status: u16, value: String) -> Self {
        Self {
            status,
            headers: std::collections::BTreeMap::new(),
            content_type: "text/plain".into(),
            body: value.into_bytes(),
        }
    }
    #[must_use]
    pub fn html(status: u16, value: String) -> Self {
        Self {
            status,
            headers: std::collections::BTreeMap::new(),
            content_type: "text/html".into(),
            body: value.into_bytes(),
        }
    }
    #[must_use]
    pub fn binary(status: u16, value: Vec<u8>) -> Self {
        Self {
            status,
            headers: std::collections::BTreeMap::new(),
            content_type: "application/octet-stream".into(),
            body: value,
        }
    }
    #[must_use]
    pub fn empty(status: u16) -> Self {
        Self {
            status,
            headers: std::collections::BTreeMap::new(),
            content_type: String::new(),
            body: Vec::new(),
        }
    }
}

/// Prefer deriving `ForklaunchResponses` on an enum: each variant owns one success
/// status/content-type pair and one representation. The native adapter validates its selected schema.
pub trait ResponseVariants: Send + 'static {
    fn responses() -> Vec<ResponseContract>;
    /// # Errors
    /// Rejects an unserializable response without exposing its value.
    fn encode(self) -> Result<EncodedResponse, ResponseEncodingError>;
}

pub struct Responses<E>(PhantomData<fn() -> E>);
impl<E: ResponseVariants> EndpointBody for Responses<E> {
    type Value = E;
    fn contract() -> Option<BodyContract> {
        None
    }
    fn response_contracts(_: u16) -> Vec<ResponseContract> {
        E::responses()
    }
    fn response_enum() -> bool {
        true
    }
}
impl<I: EndpointBody, E: ResponseVariants> Endpoint<I, Responses<E>> {
    /// # Errors
    /// Rejects duplicate/invalid variant statuses and mismatched representations.
    pub fn responses(
        name: &str,
        summary: &str,
        method: HttpMethod,
        path: &str,
    ) -> Result<Self, &'static str> {
        let mut route = RouteContract::new(name, summary, method, path);
        route.responses = E::responses();
        if let Some(body) = I::contract() {
            route = route.with_request_body(body);
        }
        if let Some(fields) = I::path_fields()? {
            route.request.params = fields;
        }
        route.request.body_optional = I::body_optional();
        route.request.query = I::query_fields()?;
        route.request.headers = I::header_fields()?;
        route.request.query_schema = I::query_schema()?;
        route.request.headers_schema = I::headers_schema()?;
        Self::from_contract(route)
    }
}

/// Response variants reject ambiguous status selection at compilation.
/// ```compile_fail
/// #[derive(forklaunch_core::contract::ForklaunchResponses)]
/// enum DuplicateStatus {
///     #[response(status = 200)] First,
///     #[response(status = 200)] Second,
/// }
/// ```
/// A media type also fixes the Rust payload representation.
/// ```compile_fail
/// #[derive(forklaunch_core::contract::ForklaunchResponses)]
/// enum InvalidText {
///     #[response(status = 200, content_type = Text)] Text(u64),
/// }
/// ```
#[allow(dead_code)]
struct ResponseCompileChecks;

/// Canonical discriminator shared by adapters: status plus normalized content type.
#[must_use]
pub fn response_identity(response: &ResponseContract) -> (u16, String) {
    (
        response.status_code,
        response
            .body
            .as_ref()
            .and_then(|body| body.content_type.as_deref())
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase(),
    )
}

/// Custom MIME overrides keep the explicit payload representation and reject controls.
/// ```compile_fail
/// #[derive(forklaunch_core::contract::ForklaunchResponses)]
/// enum HeaderInjection {
///     #[response(status=200, content_type=Text, media_type="text/plain\r\nx-secret: injected")]
///     Invalid(String),
/// }
/// ```
#[allow(dead_code)]
struct CustomMediaCompileCheck;
