//! Host-compilable endpoint descriptors shared by builds and HTTP bindings.
use crate::contract::{AccessLevel, BodyContract, ContentType, HttpMethod, RouteContract};
use std::marker::PhantomData;
mod fields;
mod headers;
mod responses;
mod versions;
pub use fields::{Fields, NoFields, RequestFields};
pub use headers::{ResponseWithHeaders, WithResponseHeaders, encode_response_headers};
pub use responses::{
    EncodedResponse, ResponseEncodingError, ResponseVariants, Responses, response_identity,
};
pub use versions::VersionedEndpoint;

/// Includes 429 and 502, which `EndpointError::TooManyRequests` / `BadGateway`
/// emit, so generated SDKs return typed failures instead of rejecting the status.
pub(super) const FRAMEWORK_FAILURE_STATUSES: [u16; 12] =
    [400, 401, 403, 404, 408, 409, 413, 415, 429, 500, 502, 503];

// Every native endpoint publishes the framework failures its admission layers can emit.
fn include_framework_failures(mut route: RouteContract) -> Result<RouteContract, &'static str> {
    for status in FRAMEWORK_FAILURE_STATUSES {
        let failure = BodyContract::json_type::<EndpointFailure>();
        if let Some(response) = route
            .responses
            .iter_mut()
            .find(|response| response_identity(response) == (status, "application/json".into()))
        {
            let body = response.body.as_mut().ok_or("missing error body")?;
            if body.schema != failure.schema {
                let domain = body
                    .schema
                    .json_schema
                    .as_deref()
                    .ok_or("typed errors need JSON schemas")?;
                let domain: serde_json::Value = serde_json::from_str(
                    &crate::contract::relocate_schema_json_unscoped(domain, &["anyOf", "0"]),
                )
                .map_err(|_| "invalid error schema")?;
                let framework: serde_json::Value = serde_json::from_str(
                    failure
                        .schema
                        .json_schema
                        .as_deref()
                        .ok_or("missing framework error schema")?,
                )
                .map_err(|_| "invalid framework error schema")?;
                body.schema = crate::contract::SchemaRef::named(format!(
                    "{}OrEndpointFailure",
                    body.schema.name
                ))
                .with_json_schema(serde_json::json!({"anyOf":[domain, framework]}).to_string());
            }
        } else {
            route = route.with_response(status, Some(failure));
        }
    }
    Ok(route)
}

/// A wire body representation. The Rust value and published schema travel together.
pub trait EndpointBody: Send + Sync + 'static {
    type Value: Send + 'static;
    fn contract() -> Option<BodyContract>;
    #[must_use]
    fn body_optional() -> bool {
        false
    }
    #[must_use]
    fn response_contracts(status: u16) -> Vec<crate::contract::ResponseContract> {
        vec![crate::contract::ResponseContract::new(
            status,
            Self::contract(),
        )]
    }
    /// # Errors
    /// Rejects optional or unsupported path field schemas.
    fn path_fields() -> Result<Option<Vec<crate::contract::FieldContract>>, &'static str> {
        Ok(None)
    }
    /// # Errors
    /// Rejects schemas that cannot be projected into query parameters.
    fn query_fields() -> Result<Vec<crate::contract::FieldContract>, &'static str> {
        Ok(Vec::new())
    }
    /// # Errors
    /// Rejects schemas that cannot be projected into HTTP headers.
    fn header_fields() -> Result<Vec<crate::contract::FieldContract>, &'static str> {
        Ok(Vec::new())
    }
    /// # Errors
    /// Rejects unsupported whole-object query schemas.
    fn query_schema() -> Result<Option<crate::contract::SchemaRef>, &'static str> {
        Ok(None)
    }
    /// # Errors
    /// Rejects unsupported whole-object header schemas.
    fn headers_schema() -> Result<Option<crate::contract::SchemaRef>, &'static str> {
        Ok(None)
    }
    #[must_use]
    fn request_only() -> bool {
        false
    }
    #[must_use]
    fn response_enum() -> bool {
        false
    }
}

/// An absent request body becomes None; present bytes still use the wrapped codec/schema.
pub struct OptionalBody<I>(PhantomData<fn() -> I>);
impl<I: EndpointBody> EndpointBody for OptionalBody<I> {
    type Value = Option<I::Value>;
    fn contract() -> Option<BodyContract> {
        I::contract()
    }
    fn body_optional() -> bool {
        true
    }
    fn request_only() -> bool {
        true
    }
}

/// A typed server-sent event stream, bound through the native streaming adapter.
pub struct SseBody<T>(PhantomData<fn() -> T>);
impl<T: schemars::JsonSchema + Send + 'static> EndpointBody for SseBody<T> {
    type Value = T;
    fn contract() -> Option<BodyContract> {
        Some(BodyContract::typed::<T>(ContentType::EventStream))
    }
}

pub struct NoBody;
impl EndpointBody for NoBody {
    type Value = ();
    fn contract() -> Option<BodyContract> {
        None
    }
}

pub struct JsonBody<T>(PhantomData<fn() -> T>);
impl<T: schemars::JsonSchema + Send + 'static> EndpointBody for JsonBody<T> {
    type Value = T;
    fn contract() -> Option<BodyContract> {
        Some(BodyContract::json_type::<T>())
    }
}

pub struct FormBody<T>(PhantomData<fn() -> T>);
impl<T: schemars::JsonSchema + Send + 'static> EndpointBody for FormBody<T> {
    type Value = T;
    fn contract() -> Option<BodyContract> {
        Some(BodyContract::typed::<T>(ContentType::Form))
    }
}
/// Multipart form fields plus file metadata. File bytes remain in typed `UploadFile` fields.
pub struct MultipartData<T> {
    pub fields: T,
    pub files: std::collections::BTreeMap<String, FileMetadata>,
}
pub struct FileMetadata {
    pub filename: String,
    pub content_type: String,
}
pub struct MultipartBody<T>(PhantomData<fn() -> T>);
impl<T: schemars::JsonSchema + Send + 'static> EndpointBody for MultipartBody<T> {
    type Value = MultipartData<T>;
    fn contract() -> Option<BodyContract> {
        Some(BodyContract::typed::<T>(ContentType::Multipart))
    }
}

pub struct BinaryBody;
impl EndpointBody for BinaryBody {
    type Value = Vec<u8>;
    fn contract() -> Option<BodyContract> {
        Some(BodyContract::typed::<String>(ContentType::Binary))
    }
}

pub struct TextBody;
pub struct HtmlBody;
macro_rules! string_body {
    ($name:ident, $content:ident) => {
        impl EndpointBody for $name {
            type Value = String;
            fn contract() -> Option<BodyContract> {
                Some(BodyContract::typed::<String>(ContentType::$content))
            }
        }
    };
}
string_body!(TextBody, Text);
string_body!(HtmlBody, Html);

#[derive(schemars::JsonSchema)]
#[allow(dead_code)]
struct EndpointFailure {
    #[allow(dead_code)]
    error: String,
}

/// A finite-body endpoint. Runtime binding must enforce this descriptor; arbitrary
/// `RouteContract` mutations cannot change the handler's request/response types.
/// Streaming uses a dedicated adapter; response enums bind each finite success variant.
pub struct Endpoint<I: EndpointBody, O: EndpointBody> {
    route: RouteContract,
    output_indices: Vec<usize>,
    marker: PhantomData<fn(I) -> O>,
}
impl<I: EndpointBody, O: EndpointBody> Clone for Endpoint<I, O> {
    fn clone(&self) -> Self {
        Self {
            route: self.route.clone(),
            output_indices: self.output_indices.clone(),
            marker: PhantomData,
        }
    }
}
impl<I: EndpointBody, O: EndpointBody> Endpoint<I, O> {
    /// Build one success response from its Rust type and wire representation.
    /// # Errors
    /// Rejects non-success or body-incompatible statuses and methods.
    pub fn new(
        name: &str,
        summary: &str,
        method: HttpMethod,
        path: &str,
        status: u16,
    ) -> Result<Self, &'static str> {
        if O::response_enum()
            || !(200..600).contains(&status)
            || (matches!(status, 204 | 205 | 304) && O::contract().is_some())
            || (method == HttpMethod::Head && O::contract().is_some())
        {
            return Err("invalid endpoint success response");
        }
        let mut route =
            RouteContract::new(name, summary, method, path).with_response(status, O::contract());
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

    /// Bind an existing policy-rich route to typed finite bodies.
    /// # Errors
    /// Rejects mismatched body/field types, duplicate response representations,
    /// versioning and optional bodies.
    pub fn from_contract(mut route: RouteContract) -> Result<Self, &'static str> {
        if O::contract().is_some_and(|body| body.kind == crate::contract::BodyKind::ServerSentEvent)
        {
            route.options.mcp = false;
        }
        if let Some(fields) = I::path_fields()? {
            let declared: std::collections::BTreeSet<_> =
                fields.iter().map(|field| field.name.as_str()).collect();
            let segments: std::collections::BTreeSet<_> = route
                .openapi_path
                .split('/')
                .filter_map(|segment| segment.strip_prefix('{')?.strip_suffix('}'))
                .collect();
            if declared != segments || route.request.params != fields {
                return Err("path fields must match route parameters");
            }
        }
        let status = route
            .responses
            .iter()
            .find(|r| (200..300).contains(&r.status_code))
            .or_else(|| route.responses.first())
            .map_or(200, |r| r.status_code);
        let expected = O::response_contracts(status);
        let unique: std::collections::BTreeSet<_> =
            expected.iter().map(response_identity).collect();
        let output_indices: Vec<_> = expected
            .iter()
            .filter_map(|response| {
                route
                    .responses
                    .iter()
                    .position(|candidate| candidate == response)
            })
            .collect();
        let declared_unique: std::collections::BTreeSet<_> =
            route.responses.iter().map(response_identity).collect();
        if declared_unique.len() != route.responses.len()
            || expected.is_empty()
            || unique.len() != expected.len()
            || output_indices.len() != expected.len()
            || route.responses.iter().any(|response| {
                (200..300).contains(&response.status_code) && !expected.contains(response)
            })
            || I::response_enum()
            || O::request_only()
            || route.request.body != I::contract().map(BodyContract::for_input)
            || route.request.body_optional != I::body_optional()
            || (I::body_optional() && I::contract().is_none())
            || !route.versions.is_empty()
            || route.request.query != I::query_fields()?
            || route.request.headers != I::header_fields()?
            || route.request.query_schema != I::query_schema()?
            || route.request.headers_schema != I::headers_schema()?
            || expected.iter().any(|response| {
                !(200..600).contains(&response.status_code)
                    || ((matches!(response.status_code, 204 | 205 | 304)
                        || route.method == HttpMethod::Head)
                        && response.body.is_some())
            })
        {
            return Err("unsupported or mismatched endpoint contract");
        }
        route = include_framework_failures(route)?;
        Ok(Self {
            route,
            output_indices,
            marker: PhantomData,
        })
    }

    /// Preserve the request value's codec while selecting its declared media type.
    /// # Errors
    /// Rejects absent bodies, incompatible codecs or unsupported MIME values.
    pub fn with_request_media(
        mut self,
        media: crate::contract::MediaType,
    ) -> Result<Self, &'static str> {
        let body = self
            .route
            .request
            .body
            .take()
            .ok_or("request has no body")?;
        self.route.request.body = Some(body.with_media_type(media)?);
        Ok(self)
    }
    /// Select media for a single-success endpoint; enums declare media on their variants.
    /// # Errors
    /// Rejects response enums, absent bodies or incompatible codecs.
    pub fn with_response_media(
        mut self,
        media: crate::contract::MediaType,
    ) -> Result<Self, &'static str> {
        if O::response_enum() {
            return Err("response enums declare media on each variant");
        }
        let response =
            &mut self.route.responses[*self.output_indices.first().ok_or("missing response")?];
        response.body = Some(
            response
                .body
                .take()
                .ok_or("response has no body")?
                .with_media_type(media)?,
        );
        Ok(self)
    }

    /// Declare a fixed response header that the native adapter applies.
    /// # Errors
    /// Rejects malformed headers and overrides of transport-managed headers.
    pub fn with_response_header(self, name: &str, value: &str) -> Result<Self, &'static str> {
        self.response_header(None, name, value)
    }
    /// Apply a fixed header only to one status/media representation.
    /// # Errors
    /// Rejects absent representations or invalid header values.
    pub fn with_response_header_for(
        self,
        status: u16,
        media: &str,
        name: &str,
        value: &str,
    ) -> Result<Self, &'static str> {
        self.response_header(Some((status, media)), name, value)
    }
    fn response_header(
        mut self,
        selected: Option<(u16, &str)>,
        name: &str,
        value: &str,
    ) -> Result<Self, &'static str> {
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
            || value.bytes().any(|byte| !(32..=126).contains(&byte))
            || matches!(
                name,
                "content-type"
                    | "content-length"
                    | "transfer-encoding"
                    | "connection"
                    | "set-cookie"
            )
        {
            return Err("invalid fixed response header");
        }
        let field = crate::contract::FieldContract {
            name: name.into(),
            required: true,
            schema: crate::contract::SchemaRef::named("FixedHeader")
                .with_json_schema(serde_json::json!({"type":"string", "const":value}).to_string()),
        };
        let mut matched = false;
        for index in &self.output_indices {
            let response = &mut self.route.responses[*index];
            if selected.is_some_and(|(status, media)| {
                response_identity(response)
                    != (
                        status,
                        media
                            .split(';')
                            .next()
                            .unwrap_or("")
                            .trim()
                            .to_ascii_lowercase(),
                    )
            }) {
                continue;
            }
            matched = true;
            response.headers.retain(|header| header.name != name);
            let mut field = field.clone();
            if FRAMEWORK_FAILURE_STATUSES.contains(&response.status_code)
                && response_identity(response).1 == "application/json"
            {
                field.required = false;
            }
            response.headers.push(field);
        }
        if !matched {
            return Err("missing response representation");
        }
        Ok(self)
    }

    #[must_use]
    pub fn with_access(mut self, access: AccessLevel) -> Self {
        self.route.access = access;
        self
    }

    #[must_use]
    pub fn output_contracts(&self) -> Vec<crate::contract::ResponseContract> {
        self.output_indices
            .iter()
            .map(|index| self.route.responses[*index].clone())
            .collect()
    }

    #[must_use]
    pub fn into_contract(self) -> RouteContract {
        self.route
    }

    #[must_use]
    pub fn contract(&self) -> RouteContract {
        self.route.clone()
    }
}

#[cfg(test)]
mod failure_projection_tests {
    use super::*;
    #[test]
    fn framework_failures_preserve_domain_error_schema() {
        let route = RouteContract::get("read", "Read", "/item")
            .with_response(200, Some(BodyContract::json_type::<String>()))
            .with_response(400, Some(BodyContract::json_type::<String>()));
        let route = include_framework_failures(route).unwrap();
        assert_eq!(route.responses.len(), 13);
        let failure = route
            .responses
            .iter()
            .find(|response| response.status_code == 400)
            .unwrap();
        let schema: serde_json::Value = serde_json::from_str(
            failure
                .body
                .as_ref()
                .unwrap()
                .schema
                .json_schema
                .as_ref()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(schema["anyOf"][0]["type"], "string");
        assert_eq!(schema["anyOf"].as_array().unwrap().len(), 2);
    }
}
