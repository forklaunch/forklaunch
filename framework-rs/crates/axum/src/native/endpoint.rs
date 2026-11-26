//! Contract-bound finite-body endpoints using Axum as the execution engine.
mod fields;
mod parser_profile;
mod responses;
#[cfg(feature = "native-streams")]
mod streams;
mod versions;
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
    routing::{MethodFilter, on},
};
use forklaunch_core::contract::{
    AccessLevel, ContractRegistry, RouteContract,
    endpoint::{
        BinaryBody, Endpoint, EndpointBody, FileMetadata, FormBody, HtmlBody, JsonBody,
        MultipartBody, MultipartData, NoBody, OptionalBody, TextBody,
    },
    schema_validation::{CheckedJsonValidator, compile_checked},
};
use serde::{Serialize, de::DeserializeOwned};
use std::{future::Future, sync::Arc};
#[cfg(feature = "native-streams")]
pub use streams::{ServerEvent, SocketSession, StreamContext, StreamOptions, VersionedSseHandlers};
pub use versions::VersionedHandlers;

/// Fixed public errors never include rejected input or schema diagnostic values.
#[derive(Clone, Copy, Debug)]
pub enum EndpointError {
    InvalidRequest,
    #[doc(hidden)]
    RequestSchemaAssertion,
    UnsupportedContentType,
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    Unavailable,
    InvalidResponse,
    PayloadTooLarge,
    /// An upstream allowance was reached (for example, the model gateway's 429).
    TooManyRequests,
    /// An upstream call failed or was interrupted.
    BadGateway,
}
impl IntoResponse for EndpointError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::InvalidRequest | Self::RequestSchemaAssertion => {
                (StatusCode::BAD_REQUEST, "invalid request")
            }
            Self::UnsupportedContentType => (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported content type",
            ),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "authentication required"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "access denied"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not found"),
            Self::Conflict => (StatusCode::CONFLICT, "resource changed"),
            Self::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "service unavailable"),
            Self::InvalidResponse => (StatusCode::INTERNAL_SERVER_ERROR, "invalid response"),
            Self::PayloadTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "request body too large"),
            Self::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, "too many requests"),
            Self::BadGateway => (StatusCode::BAD_GATEWAY, "upstream request failed"),
        };
        (status, axum::Json(serde_json::json!({"error": message}))).into_response()
    }
}

/// Explicit security work implemented by a policy. This is an implementation contract,
/// not evidence that an arbitrary application policy performs that work correctly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PolicyCapabilities {
    #[default]
    Authenticated,
    Authorized,
    SignedInternal,
    AuthorizedSignedInternal,
}
impl PolicyCapabilities {
    fn admit(self, route: &RouteContract) -> Result<(), String> {
        let signed = matches!(self, Self::SignedInternal | Self::AuthorizedSignedInternal);
        let authorized = matches!(self, Self::Authorized | Self::AuthorizedSignedInternal);
        if (route.access == AccessLevel::Internal
            || matches!(
                route.auth,
                Some(forklaunch_core::contract::AuthConfig::Hmac(_))
            ))
            && !signed
        {
            return Err(
                "internal/HMAC endpoint requires an original-body signature and replay policy"
                    .into(),
            );
        }
        if (route.access == AccessLevel::Protected || route.rbac.is_some()) && !authorized {
            return Err("protected endpoint requires an authorization policy".into());
        }
        Ok(())
    }
}

/// Explicit authorization hook. Resolve trusted session/tenant state here, before
/// body parsing or handler execution. The same mounted router services MCP calls.
pub trait EndpointPolicy<S>: Clone + Send + Sync + 'static {
    /// Opt in only when the policy implements the declared checks. Signed policies
    /// must verify original bytes and durable replay admission in `authorize_body`.
    const CAPABILITIES: PolicyCapabilities = PolicyCapabilities::Authenticated;
    type Context: Send + 'static;
    fn authorize(
        &self,
        parts: &mut Parts,
        state: &S,
        route: &RouteContract,
    ) -> impl Future<Output = Result<Self::Context, EndpointError>> + Send;
    /// Verify signatures, decrypt envelopes or claim replay protection using bounded
    /// original wire bytes. Runs before path/media/schema decoding and the handler.
    /// # Errors
    /// Rejects unverified body credentials without exposing payload values.
    fn authorize_body(
        &self,
        context: Self::Context,
        _parts: &mut Parts,
        _bytes: &[u8],
        _state: &S,
        _route: &RouteContract,
    ) -> impl Future<Output = Result<Self::Context, EndpointError>> + Send {
        std::future::ready(Ok(context))
    }
}
#[derive(Clone)]
struct Public;
impl<S: Sync> EndpointPolicy<S> for Public {
    type Context = ();
    fn authorize(
        &self,
        _: &mut Parts,
        _: &S,
        route: &RouteContract,
    ) -> impl Future<Output = Result<(), EndpointError>> + Send {
        std::future::ready(
            if route.access == AccessLevel::Public && route.auth.is_none() && route.rbac.is_none() {
                Ok(())
            } else {
                Err(EndpointError::Forbidden)
            },
        )
    }
}

/// Supported native body codecs; values are never exposed in diagnostic errors.
pub trait NativeBody: EndpointBody {
    /// # Errors
    /// Rejects malformed payloads or schema constraint violations.
    fn decode<'a>(
        bytes: &'a [u8],
        content_type: &'a str,
        validator: Option<&'a CheckedJsonValidator>,
    ) -> impl Future<Output = Result<Self::Value, EndpointError>> + Send + 'a;
    /// # Errors
    /// Rejects invalid body or typed request metadata.
    #[must_use = "the decode future does nothing unless awaited"]
    fn decode_request<'a>(
        bytes: &'a [u8],
        content_type: &'a str,
        validator: Option<&'a CheckedJsonValidator>,
        _query: serde_json::Value,
        _headers: serde_json::Value,
        _path: serde_json::Value,
    ) -> impl Future<Output = Result<Self::Value, EndpointError>> + Send + 'a {
        Self::decode(bytes, content_type, validator)
    }
    /// Decode a framework-parsed body without applying another wire parser.
    /// # Errors
    /// Rejects unsupported custom codecs and schema/type violations.
    fn decode_parsed(
        _value: serde_json::Value,
        _validator: Option<&CheckedJsonValidator>,
    ) -> Result<Self::Value, EndpointError> {
        Err(EndpointError::InvalidRequest)
    }
    /// Decode a body profile together with typed metadata.
    /// # Errors
    /// Rejects schema/type violations or unsupported custom codecs.
    #[must_use = "the decode future does nothing unless awaited"]
    fn decode_profile_request<'a>(
        bytes: &'a [u8],
        content_type: &'a str,
        validator: Option<&'a CheckedJsonValidator>,
        parsed: Option<serde_json::Value>,
        metadata: (serde_json::Value, serde_json::Value, serde_json::Value),
    ) -> impl Future<Output = Result<Self::Value, EndpointError>> + Send + 'a {
        async move {
            if let Some(value) = parsed {
                Self::decode_parsed(value, validator)
            } else {
                Self::decode_request(
                    bytes,
                    content_type,
                    validator,
                    metadata.0,
                    metadata.1,
                    metadata.2,
                )
                .await
            }
        }
    }
    /// # Errors
    /// Rejects serialization failures or schema constraint violations.
    fn encode(
        value: Self::Value,
        validator: Option<&CheckedJsonValidator>,
    ) -> Result<Vec<u8>, EndpointError>;
    /// # Errors
    /// Rejects serialization or schema failures; response enums select their own declared status.
    fn encode_response(
        value: Self::Value,
        status: u16,
        validator: Option<&CheckedJsonValidator>,
    ) -> Result<forklaunch_core::contract::endpoint::EncodedResponse, EndpointError> {
        Ok(forklaunch_core::contract::endpoint::EncodedResponse {
            status,
            headers: std::collections::BTreeMap::new(),
            content_type: Self::contract()
                .and_then(|body| body.content_type)
                .unwrap_or_default()
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase(),
            body: Self::encode(value, validator)?,
        })
    }
}
impl<I: NativeBody> NativeBody for OptionalBody<I> {
    async fn decode(
        bytes: &[u8],
        content_type: &str,
        validator: Option<&CheckedJsonValidator>,
    ) -> Result<Self::Value, EndpointError> {
        if bytes.is_empty() {
            Ok(None)
        } else {
            I::decode(bytes, content_type, validator).await.map(Some)
        }
    }
    fn decode_parsed(
        value: serde_json::Value,
        validator: Option<&CheckedJsonValidator>,
    ) -> Result<Self::Value, EndpointError> {
        I::decode_parsed(value, validator).map(Some)
    }
    fn encode(_: Self::Value, _: Option<&CheckedJsonValidator>) -> Result<Vec<u8>, EndpointError> {
        Err(EndpointError::InvalidResponse)
    }
}
fn request_schema_error(
    error: &forklaunch_core::contract::schema_validation::SchemaValidationFailure,
) -> EndpointError {
    match error {
        forklaunch_core::contract::schema_validation::SchemaValidationFailure::Invalid(_) => {
            EndpointError::RequestSchemaAssertion
        }
        forklaunch_core::contract::schema_validation::SchemaValidationFailure::Operational => {
            EndpointError::InvalidRequest
        }
    }
}
fn decode_json_value<T: DeserializeOwned>(
    value: serde_json::Value,
    validator: Option<&CheckedJsonValidator>,
) -> Result<T, EndpointError> {
    if let Some(validate) = validator {
        validate(&value).map_err(|error| request_schema_error(&error))?;
    }
    serde_json::from_value(value).map_err(|_| EndpointError::InvalidRequest)
}
// Finite codecs share the asynchronous extractor contract with multipart.
#[allow(clippy::unused_async_trait_impl)]
impl NativeBody for NoBody {
    async fn decode(
        bytes: &[u8],
        _content_type: &str,
        _: Option<&CheckedJsonValidator>,
    ) -> Result<(), EndpointError> {
        if bytes.is_empty() {
            Ok(())
        } else {
            Err(EndpointError::InvalidRequest)
        }
    }
    fn encode((): (), _: Option<&CheckedJsonValidator>) -> Result<Vec<u8>, EndpointError> {
        Ok(Vec::new())
    }
}
#[allow(clippy::unused_async_trait_impl)]
impl<T> NativeBody for JsonBody<T>
where
    T: forklaunch_core::contract::__private::schemars::JsonSchema
        + DeserializeOwned
        + Serialize
        + Send
        + 'static,
{
    async fn decode(
        bytes: &[u8],
        _content_type: &str,
        validator: Option<&CheckedJsonValidator>,
    ) -> Result<T, EndpointError> {
        let value = serde_json::from_slice(bytes).map_err(|_| EndpointError::InvalidRequest)?;
        if let Some(validate) = validator {
            validate(&value).map_err(|error| request_schema_error(&error))?;
        }
        serde_json::from_value(value).map_err(|_| EndpointError::InvalidRequest)
    }
    fn decode_parsed(
        value: serde_json::Value,
        validator: Option<&CheckedJsonValidator>,
    ) -> Result<Self::Value, EndpointError> {
        decode_json_value(value, validator)
    }
    fn encode(
        value: T,
        validator: Option<&CheckedJsonValidator>,
    ) -> Result<Vec<u8>, EndpointError> {
        let value = serde_json::to_value(value).map_err(|_| EndpointError::InvalidResponse)?;
        if let Some(validate) = validator {
            validate(&value).map_err(|_| EndpointError::InvalidResponse)?;
        }
        serde_json::to_vec(&value).map_err(|_| EndpointError::InvalidResponse)
    }
}
#[allow(clippy::unused_async_trait_impl)]
impl<T> NativeBody for FormBody<T>
where
    T: forklaunch_core::contract::__private::schemars::JsonSchema
        + DeserializeOwned
        + Serialize
        + Send
        + 'static,
{
    async fn decode(
        bytes: &[u8],
        content_type: &str,
        validator: Option<&CheckedJsonValidator>,
    ) -> Result<T, EndpointError> {
        let value: T =
            forklaunch_core::http::decode_urlencoded(bytes, content_type).map_err(|error| {
                match error.location {
                    forklaunch_core::http::ContractValidationLocation::BodySize => {
                        EndpointError::PayloadTooLarge
                    }
                    forklaunch_core::http::ContractValidationLocation::BodyEncoding => {
                        EndpointError::UnsupportedContentType
                    }
                    _ => EndpointError::InvalidRequest,
                }
            })?;
        let json = serde_json::to_value(&value).map_err(|_| EndpointError::InvalidRequest)?;
        if let Some(validate) = validator {
            validate(&json).map_err(|error| request_schema_error(&error))?;
        }
        Ok(value)
    }
    fn decode_parsed(
        value: serde_json::Value,
        validator: Option<&CheckedJsonValidator>,
    ) -> Result<Self::Value, EndpointError> {
        let decoded: T = forklaunch_core::http::decode_form_value(value)
            .map_err(|_| EndpointError::InvalidRequest)?;
        let value = serde_json::to_value(&decoded).map_err(|_| EndpointError::InvalidRequest)?;
        if let Some(validate) = validator {
            validate(&value).map_err(|error| request_schema_error(&error))?;
        }
        Ok(decoded)
    }
    fn encode(
        value: T,
        validator: Option<&CheckedJsonValidator>,
    ) -> Result<Vec<u8>, EndpointError> {
        let json = serde_json::to_value(&value).map_err(|_| EndpointError::InvalidResponse)?;
        if let Some(validate) = validator {
            validate(&json).map_err(|_| EndpointError::InvalidResponse)?;
        }
        serde_urlencoded::to_string(value)
            .map(String::into_bytes)
            .map_err(|_| EndpointError::InvalidResponse)
    }
}
#[allow(clippy::unused_async_trait_impl)]
impl NativeBody for BinaryBody {
    async fn decode(
        bytes: &[u8],
        _content_type: &str,
        _: Option<&CheckedJsonValidator>,
    ) -> Result<Vec<u8>, EndpointError> {
        Ok(bytes.to_vec())
    }
    fn decode_parsed(
        value: serde_json::Value,
        validator: Option<&CheckedJsonValidator>,
    ) -> Result<Self::Value, EndpointError> {
        decode_json_value(value, validator)
    }
    fn encode(value: Vec<u8>, _: Option<&CheckedJsonValidator>) -> Result<Vec<u8>, EndpointError> {
        Ok(value)
    }
}

impl<T> NativeBody for MultipartBody<T>
where
    T: forklaunch_core::contract::__private::schemars::JsonSchema
        + DeserializeOwned
        + Send
        + 'static,
{
    async fn decode(
        bytes: &[u8],
        content_type: &str,
        validator: Option<&CheckedJsonValidator>,
    ) -> Result<MultipartData<T>, EndpointError> {
        use axum::extract::FromRequest;
        let request = Request::builder()
            .method("POST")
            .header("content-type", content_type)
            .body(Body::from(bytes.to_vec()))
            .map_err(|_| EndpointError::InvalidRequest)?;
        let mut multipart = axum::extract::Multipart::from_request(request, &())
            .await
            .map_err(|_| EndpointError::InvalidRequest)?;
        let mut values = serde_json::Map::new();
        let mut files = std::collections::BTreeMap::new();
        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|_| EndpointError::InvalidRequest)?
        {
            let name = field
                .name()
                .ok_or(EndpointError::InvalidRequest)?
                .to_owned();
            if values.contains_key(&name) || values.len() >= 64 {
                return Err(EndpointError::InvalidRequest);
            }
            let filename = field.file_name().map(str::to_owned);
            let content_type = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_owned();
            let bytes = field
                .bytes()
                .await
                .map_err(|_| EndpointError::InvalidRequest)?;
            if bytes.len() > 1024 * 1024 {
                return Err(EndpointError::PayloadTooLarge);
            }
            let value = if let Some(filename) = filename {
                files.insert(
                    name.clone(),
                    FileMetadata {
                        filename,
                        content_type,
                    },
                );
                serde_json::to_value(bytes.to_vec()).map_err(|_| EndpointError::InvalidRequest)?
            } else {
                serde_json::Value::String(
                    String::from_utf8(bytes.to_vec()).map_err(|_| EndpointError::InvalidRequest)?,
                )
            };
            values.insert(name, value);
        }
        let value = serde_json::Value::Object(values);
        if let Some(validate) = validator {
            validate(&value).map_err(|error| request_schema_error(&error))?;
        }
        let fields = serde_json::from_value(value).map_err(|_| EndpointError::InvalidRequest)?;
        Ok(MultipartData { fields, files })
    }
    fn encode(
        _: MultipartData<T>,
        _: Option<&CheckedJsonValidator>,
    ) -> Result<Vec<u8>, EndpointError> {
        Err(EndpointError::InvalidResponse)
    }
}

macro_rules! string_codec {
    ($kind:ty) => {
        #[allow(clippy::unused_async_trait_impl)]
        impl NativeBody for $kind {
            async fn decode(
                bytes: &[u8],
                _content_type: &str,
                _: Option<&CheckedJsonValidator>,
            ) -> Result<String, EndpointError> {
                String::from_utf8(bytes.to_vec()).map_err(|_| EndpointError::InvalidRequest)
            }
            fn decode_parsed(
                value: serde_json::Value,
                validator: Option<&CheckedJsonValidator>,
            ) -> Result<Self::Value, EndpointError> {
                decode_json_value(value, validator)
            }
            fn encode(
                value: String,
                _: Option<&CheckedJsonValidator>,
            ) -> Result<Vec<u8>, EndpointError> {
                Ok(value.into_bytes())
            }
        }
    };
}
string_codec!(TextBody);
string_codec!(HtmlBody);

/// Routes are checked against the exact host-built registry.
/// Handler result types must match the declared wire body:
/// ```compile_fail
/// use forklaunch_axum::native::ContractRouter;
/// use forklaunch_core::contract::{ContractRegistry, HttpMethod, endpoint::{Endpoint, NoBody, TextBody}};
/// let endpoint = Endpoint::<NoBody, TextBody>::new("hello", "hello", HttpMethod::Get, "/hello", 200).unwrap();
/// let mut registry = ContractRegistry::new();
/// registry.register(endpoint.contract());
/// let _ = ContractRouter::<()>::new(registry).bind(endpoint, |_, ()| async { Ok(42_u32) });
/// ```
/// Finishing fails if a
/// contract is missing a binding, rather than advertising nonexistent SDK methods.
pub struct ContractRouter<S> {
    router: Router<S>,
    registry: ContractRegistry,
    mounted: Vec<usize>,
    mounted_sockets: Vec<String>,
}
impl<S: Clone + Send + Sync + 'static> ContractRouter<S> {
    #[must_use]
    pub fn new(registry: ContractRegistry) -> Self {
        Self {
            router: Router::new(),
            registry,
            mounted: Vec::new(),
            mounted_sockets: Vec::new(),
        }
    }

    /// Bind a public handler. Private contracts require `bind_authorized`.
    /// # Errors
    /// Rejects private, duplicate, absent, or divergent contract bindings.
    pub fn bind<I, O, F, Fut>(self, endpoint: Endpoint<I, O>, handler: F) -> Result<Self, String>
    where
        I: NativeBody,
        O: NativeBody,
        F: Fn(State<S>, I::Value) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<O::Value, EndpointError>> + Send + 'static,
    {
        let declared = endpoint.contract();
        if declared.access != AccessLevel::Public
            || declared.auth.is_some()
            || declared.rbac.is_some()
        {
            return Err("private endpoint requires an authorization policy".into());
        }
        self.bind_authorized(endpoint, Public, move |state, (), body| {
            handler(state, body)
        })
    }

    /// Bind a policy that supplies authenticated application context to the handler.
    /// # Errors
    /// Rejects duplicate, missing, divergent, or unsupported contracts at startup.
    // Keep contract admission and its captured runtime enforcement together.
    #[allow(clippy::too_many_lines)]
    pub fn bind_authorized<I, O, P, F, Fut>(
        mut self,
        endpoint: Endpoint<I, O>,
        policy: P,
        handler: F,
    ) -> Result<Self, String>
    where
        I: NativeBody,
        O: NativeBody,
        P: EndpointPolicy<S>,
        F: Fn(State<S>, P::Context, I::Value) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<O::Value, EndpointError>> + Send + 'static,
    {
        let outputs = endpoint.output_contracts();
        let route = endpoint.into_contract();
        P::CAPABILITIES.admit(&route)?;
        let candidates: Vec<_> = self
            .registry
            .routes()
            .iter()
            .enumerate()
            .filter(|(_, candidate)| {
                candidate.method == route.method && candidate.path == route.path
            })
            .collect();
        if candidates.len() != 1 || candidates[0].1 != &route {
            return Err("endpoint differs from published contract".into());
        }
        let index = candidates[0].0;
        if self.mounted.contains(&index) {
            return Err("endpoint already bound".into());
        }
        let method: axum::http::Method = route
            .method
            .as_str()
            .parse()
            .map_err(|_| "unsupported method")?;
        let filter = MethodFilter::try_from(method).map_err(|_| "unsupported native method")?;
        let path = route.openapi_path.clone();
        let execute = compile_handler::<S, I, O, P::Context, F, Fut>(&route, &outputs, handler)?;
        let route = Arc::new(route);
        let mounted = on(filter, move |State(state): State<S>, request: Request| {
            let policy = policy.clone();
            let route = route.clone();
            let execute = execute.clone();
            async move {
                let (mut parts, body) = request.into_parts();
                let context = policy.authorize(&mut parts, &state, &route).await?;
                let bytes = to_bytes(body, 2 * 1024 * 1024)
                    .await
                    .map_err(|_| EndpointError::PayloadTooLarge)?;
                let context = policy
                    .authorize_body(context, &mut parts, &bytes, &state, &route)
                    .await?;
                execute(state, context, parts, bytes).await
            }
        });
        self.router = self.router.route(&path, mounted);
        self.mounted.push(index);
        Ok(self)
    }

    /// Finish after every published route has a handler.
    /// # Errors
    /// Rejects unbound published routes.
    pub fn finish(self) -> Result<Router<S>, String> {
        if self.mounted.len() != self.registry.routes().len()
            || self.mounted_sockets.len() != self.registry.socket_contracts().len()
        {
            return Err("published endpoint has no handler".into());
        }
        Ok(self.router)
    }
}
type BoundHandler<S, C> = Arc<
    dyn Fn(
            S,
            C,
            Parts,
            axum::body::Bytes,
        )
            -> std::pin::Pin<Box<dyn Future<Output = Result<Response, EndpointError>> + Send>>
        + Send
        + Sync,
>;
fn compile_handler<S, I, O, C, F, Fut>(
    route: &RouteContract,
    outputs: &[forklaunch_core::contract::ResponseContract],
    handler: F,
) -> Result<BoundHandler<S, C>, String>
where
    S: Clone + Send + Sync + 'static,
    C: Send + 'static,
    I: NativeBody,
    O: NativeBody,
    F: Fn(State<S>, C, I::Value) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<O::Value, EndpointError>> + Send + 'static,
{
    let first_output = outputs.first().ok_or("missing response")?;
    let default_identity = forklaunch_core::contract::endpoint::response_identity(first_output);
    let status =
        StatusCode::from_u16(first_output.status_code).map_err(|_| "invalid response status")?;
    let response_plans = Arc::new(responses::configure(responses::plans(outputs)?, route));
    let request_plan = Arc::new(RequestPlan::new(route)?);

    Ok(Arc::new(move |state, context, mut parts, bytes| {
        let response_plans = response_plans.clone();
        let default_identity = default_identity.clone();
        let request_plan = request_plan.clone();
        let handler = handler.clone();
        Box::pin(async move {
            let matched = parts.extensions.remove::<responses::MatchedResponsePlans>();
            let input = request_plan
                .decode::<I, S>(&mut parts, &state, &bytes)
                .await?;
            let value = handler(State(state), context, input).await?;

            let mut encoded = O::encode_response(value, status.as_u16(), None)?;
            if !O::response_enum() {
                encoded.content_type.clone_from(&default_identity.1);
            }
            let identity = (encoded.status, encoded.content_type);
            let plan = response_plans
                .get(&identity)
                .ok_or(EndpointError::InvalidResponse)?;
            match matched {
                Some(matched) => matched.render(plan, &identity, encoded.body, encoded.headers),
                None => plan.render(encoded.body, encoded.headers),
            }
        })
    }))
}

struct RequestPlan {
    request_mode: forklaunch_core::contract::ValidationMode,
    operation: String,
    parsers: forklaunch_core::contract::BodyParserOptions,
    body: Option<forklaunch_core::contract::BodyContract>,
    validator: Option<Arc<CheckedJsonValidator>>,
    fields: fields::FieldPlans,
}
impl RequestPlan {
    fn new(route: &RouteContract) -> Result<Self, String> {
        forklaunch_core::http::validate_parser_options(&route.options.body_parser)?;
        Ok(Self {
            request_mode: route.options.request_validation,
            operation: route.operation_name.clone(),
            parsers: route.options.body_parser.clone(),
            body: route.request.body.clone(),
            validator: validator(route.request.body.as_ref())?,
            fields: fields::FieldPlans::new(
                &route.request.params,
                &route.request.query,
                &route.request.headers,
                route.request.query_schema.as_ref(),
                route.request.headers_schema.as_ref(),
            )?,
        })
    }
    async fn decode<I: NativeBody, S: Send + Sync>(
        &self,
        parts: &mut Parts,
        state: &S,
        bytes: &[u8],
    ) -> Result<I::Value, EndpointError> {
        let path = if self.fields.has_path() {
            use axum::extract::FromRequestParts;
            let axum::extract::Path(values) = axum::extract::Path::<
                std::collections::HashMap<String, String>,
            >::from_request_parts(parts, state)
            .await
            .map_err(|_| EndpointError::InvalidRequest)?;
            values
        } else {
            std::collections::HashMap::new()
        };
        if let Some(expected) = self
            .body
            .as_ref()
            .and_then(|body| body.content_type.as_ref())
            && !(I::body_optional() && bytes.is_empty())
        {
            let actual = parts
                .headers
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("");
            if !actual
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case(expected.split(';').next().unwrap_or(""))
            {
                return Err(EndpointError::UnsupportedContentType);
            }
        }
        let content_type = parts
            .headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        self.decode_with_mode::<I>(parts, bytes, content_type, &path)
            .await
    }
}

fn validator(
    body: Option<&forklaunch_core::contract::BodyContract>,
) -> Result<Option<Arc<CheckedJsonValidator>>, String> {
    body.and_then(|body| body.schema.json_schema.as_ref())
        .map(|schema| {
            let schema = serde_json::from_str(schema).map_err(|_| "invalid endpoint schema")?;
            if body.is_some_and(|body| body.kind == forklaunch_core::contract::BodyKind::Multipart)
            {
                forklaunch_core::contract::schema_validation::compile_multipart_checked(&schema)
                    .map_err(|_| "invalid endpoint schema".into())
            } else {
                compile_checked(&schema).map_err(|_| "invalid endpoint schema".into())
            }
        })
        .transpose()
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod normalization_tests;

#[cfg(test)]
#[path = "whole_field_tests.rs"]
mod whole_field_tests;

#[cfg(test)]
mod response_modes_tests;

#[cfg(test)]
mod request_modes_tests;
