//! Declaration-ordered request matching for versioned HTTP contracts.
use crate::http::{
    Body, ContractValidationError, ContractValidationLocation, FrameworkRequest,
    validate_request_body,
};
use forklaunch_core::contract::{
    BodyKind, RouteContract, SchemaRef,
    schema_validation::{self, CheckedJsonValidator, SchemaValidationFailure},
};
use serde_json::Value;
use std::{borrow::Cow, sync::Arc};
#[path = "versioning_hooks.rs"]
mod hooks;
#[path = "versioning_request_parser.rs"]
mod request_parser;
pub use request_parser::{
    VersionRequestInput, VersionRequestJson, VersionRequestJsonError, VersionRequestParseFailure,
    VersionRequestParser, VersionRequestProjection,
};
#[path = "versioning_parser_hooks.rs"]
mod parser_hooks;
pub use hooks::{VersionBodyHooks, VersionBodyHooksRegistry};
#[path = "versioning_fields.rs"]
mod fields;
pub use fields::{VersionFieldHooks, VersionFieldValues};
pub use parser_hooks::BodyParserHooks;
#[path = "versioning_charset.rs"]
mod charset;
#[path = "versioning_form.rs"]
mod form;
#[path = "versioning_multipart.rs"]
mod multipart;
#[path = "versioning_wire.rs"]
mod wire;
pub use multipart::MultipartLimits;
#[path = "versioning_json_request.rs"]
mod json_request;
pub use json_request::{JsonRequestParsers, ParsedJsonBody};
#[path = "versioning_wire_profile.rs"]
mod wire_profile;
use wire_profile::VersionWireParser;

#[path = "versioning_prepared.rs"]
mod prepared;
pub use prepared::PreparedVersionSelection;

/// First parser output, independent of version schema matching. JSON/text wire
/// profiles retain decompressed bytes; legacy parsers under a trusted whole-request
/// parser retain only the parsed value, with no fabricated raw-byte projection.
/// Original wire bytes remain on the request; this projection is read-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionParsedBody(ParsedJsonBody);
impl VersionParsedBody {
    /// Parsed value; absence includes media/framing skips and reviver root removal.
    #[must_use]
    pub fn value(&self) -> Option<&Value> {
        self.0.value.as_deref()
    }
    /// Decompressed parser input; absent when parsing was skipped.
    #[must_use]
    pub fn raw_body(&self) -> Option<&[u8]> {
        self.0.raw_body.as_deref().map(Vec::as_slice)
    }
}
type Selection = Result<Option<(RouteContract, SelectedVersion)>, Vec<ContractValidationError>>;
#[derive(Clone)]
struct SelectionOutcome {
    selection: Selection,
    parsed: Option<VersionParsedBody>,
}

/// Available to local handlers and as an Axum request extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedVersion {
    pub version: String,
    pub matched_versions: Vec<String>,
    /// Parsed once for selection; original wire bytes remain on the request.
    pub parsed_body: Option<Arc<Value>>,
    pub parsed_fields: Arc<VersionFieldValues>,
}

#[derive(Clone)]
struct FieldValidator {
    location: ContractValidationLocation,
    name: String,
    validate: Option<Arc<CheckedJsonValidator>>,
    required: bool,
    hooks: VersionFieldHooks,
}
#[derive(Clone)]
struct Variant {
    name: String,
    route: RouteContract,
    fields: Vec<FieldValidator>,
    query_group: Option<Arc<CheckedJsonValidator>>,
    headers_group: Option<Arc<CheckedJsonValidator>>,
    body: Option<BodyValidator>,
    hooks: VersionBodyHooks,
}

#[derive(Clone)]
enum BodyValidator {
    Value(Arc<CheckedJsonValidator>),
    Bytes(Arc<CheckedJsonValidator>),
    Opaque,
}

/// Reusable schema plan; live routers compile it once during construction.
#[derive(Default, Clone)]
pub struct VersionedRoutePlan {
    variants: Vec<Variant>,
    request_parsers: Option<Arc<std::collections::BTreeMap<String, VersionRequestParser>>>,
    multipart_text: Option<crate::http::BusboyTextOptions>,
    identity: Arc<()>,
    typescript_simple_query: bool,
    typescript_wire_parser: Option<VersionWireParser>,
    response_plan: crate::http::versioned_response::VersionResponsePlan,
    parsers: forklaunch_core::contract::BodyParserOptions,
    parser_hooks: BodyParserHooks,
}
impl std::fmt::Debug for VersionedRoutePlan {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VersionedRoutePlan")
            .field("variant_count", &self.variants.len())
            .field("parsers", &self.parsers)
            .field("parser_hooks", &self.parser_hooks)
            .finish_non_exhaustive()
    }
}
impl VersionedRoutePlan {
    /// # Errors
    /// Rejects invalid embedded schemas or excessive version counts.
    pub fn compile(route: &RouteContract) -> Result<Self, String> {
        if route.versions.is_empty()
            && (route.request.query_schema.is_some() || route.request.headers_schema.is_some())
        {
            return Err(
                "unversioned whole query/header schemas require the native typed router".into(),
            );
        }
        if route.versions.len() > 1024 {
            return Err("route exceeds 1024 versions".into());
        }
        let parsers = route.options.body_parser.clone();
        if !route.versions.is_empty() {
            validate_parser_options(&parsers)?;
        }
        let parser = parser_kind(route.parsing_body());
        let mut remaining_bytes = 32 * 1024 * 1024_usize;
        let mut variants = Vec::new();
        for name in route.versions.keys() {
            let projected = route.for_version(name).ok_or("missing indexed version")?;
            if parser_kind(projected.request.body.as_ref()) != parser {
                return Err("All API versions must use the same body parser".into());
            }
            if parser == Some(BodyKind::ServerSentEvent) {
                return Err("SSE request bodies are not supported".into());
            }
            let mut fields = Vec::new();
            for (location, entries) in [
                (
                    ContractValidationLocation::PathParameter,
                    &projected.request.params,
                ),
                (ContractValidationLocation::Query, &projected.request.query),
                (
                    ContractValidationLocation::Header,
                    &projected.request.headers,
                ),
            ] {
                for field in entries {
                    fields.push(FieldValidator {
                        location,
                        name: field.name.clone(),
                        validate: compile_schema(&field.schema, &mut remaining_bytes)?,
                        required: field.required,
                        hooks: VersionFieldHooks::default(),
                    });
                }
            }
            let body = projected
                .request
                .body
                .as_ref()
                .map(|body| compile_body(body, &mut remaining_bytes))
                .transpose()?
                .flatten();
            let query_group = compile_group(
                projected.request.query_schema.as_ref(),
                &mut remaining_bytes,
            )?;
            let headers_group = compile_group(
                projected.request.headers_schema.as_ref(),
                &mut remaining_bytes,
            )?;
            variants.push(Variant {
                name: name.clone(),
                route: projected,
                fields,
                query_group,
                headers_group,
                body,
                hooks: VersionBodyHooks::default(),
            });
        }
        Ok(Self {
            response_plan: crate::http::versioned_response::VersionResponsePlan::compile(route),
            variants,
            request_parsers: None,
            multipart_text: None,
            identity: Arc::new(()),
            typescript_simple_query: false,
            typescript_wire_parser: None,
            parsers,
            parser_hooks: BodyParserHooks::default(),
        })
    }

    /// Shared response schemas for actual matched versions, without rerunning hooks.
    #[must_use]
    pub fn matched_response_versions(
        &self,
        selected: &SelectedVersion,
    ) -> Option<crate::http::MatchedVersionResponses> {
        self.response_plan.select(selected)
    }

    /// Install callbacks for the shared body parser.
    /// # Errors
    /// Rejects routes without a supported body parser.
    pub fn with_parser_hooks(mut self, hooks: BodyParserHooks) -> Result<Self, String> {
        let body = self
            .variants
            .first()
            .and_then(|variant| variant.route.request.body.as_ref())
            .ok_or("Parser hooks require a declared body")?;
        if matches!(body.kind, BodyKind::Multipart | BodyKind::ServerSentEvent) {
            return Err("This body kind does not use body-parser verification".into());
        }
        if hooks.has_json_reviver() && !matches!(body.kind, BodyKind::Json | BodyKind::Unknown) {
            return Err("JSON reviver requires a JSON body parser".into());
        }
        self.parser_hooks = hooks;
        self.identity = Arc::new(());
        match &self.typescript_wire_parser {
            Some(VersionWireParser::Json(_)) => return self.with_typescript_json_wire(),
            Some(VersionWireParser::Text { .. }) => return self.with_typescript_text_wire(),
            None => {}
        }
        Ok(self)
    }

    /// # Errors
    /// Rejects unknown versions or hooks on a version without a declared body.
    pub fn with_body_hooks(
        mut self,
        version: &str,
        hooks: VersionBodyHooks,
    ) -> Result<Self, String> {
        if self.request_parsers.is_some() {
            return Err("Whole-request parsers cannot combine with body/field hooks".into());
        }
        let variant = self
            .variants
            .iter_mut()
            .find(|variant| variant.name == version)
            .ok_or("Unknown hook version")?;
        if variant.route.request.body.is_none() {
            return Err("Body hooks require a declared body".into());
        }
        variant.hooks = hooks;
        self.identity = Arc::new(());
        Ok(self)
    }

    /// Install value callbacks for a declared field in one version.
    /// # Errors
    /// Rejects unknown versions, locations and field names.
    pub fn with_field_hooks(
        mut self,
        version: &str,
        location: ContractValidationLocation,
        name: &str,
        hooks: VersionFieldHooks,
    ) -> Result<Self, String> {
        if self.request_parsers.is_some() {
            return Err("Whole-request parsers cannot combine with body/field hooks".into());
        }
        let variant = self
            .variants
            .iter_mut()
            .find(|variant| variant.name == version)
            .ok_or("Unknown hook version")?;
        let field = variant
            .fields
            .iter_mut()
            .find(|field| {
                field.location == location
                    && if location == ContractValidationLocation::Header {
                        field.name.eq_ignore_ascii_case(name)
                    } else {
                        field.name == name
                    }
            })
            .ok_or("Field hooks require a declared parameter, query or header field")?;
        field.hooks = hooks;
        self.identity = Arc::new(());
        Ok(self)
    }

    /// Return the last matching declaration, retaining every matching name.
    /// Validation options control rejection at the caller, not version discovery.
    /// # Errors
    /// Returns a validation error when no declared version accepts the request.
    pub fn select(
        &self,
        request: &FrameworkRequest,
    ) -> Result<Option<(RouteContract, SelectedVersion)>, Vec<ContractValidationError>> {
        self.select_with_multipart_limits(request, MultipartLimits::default())
    }

    fn with_typescript_json_wire(mut self) -> Result<Self, String> {
        let first = self
            .variants
            .first()
            .ok_or("JSON wire profile requires versions")?;
        let mut parser = JsonRequestParsers::default();
        parser.insert(&first.route, self.parser_hooks.clone())?;
        self.typescript_wire_parser = Some(VersionWireParser::Json(parser));
        self.identity = Arc::new(());
        Ok(self)
    }

    fn with_typescript_text_wire(mut self) -> Result<Self, String> {
        let first = self
            .variants
            .first()
            .ok_or("Text wire profile requires versions")?;
        self.typescript_wire_parser = Some(VersionWireParser::text(
            &first.route,
            self.parser_hooks.clone(),
        )?);
        self.identity = Arc::new(());
        Ok(self)
    }

    /// Opt into Busboy text decoding for a multipart version plan.
    /// # Errors
    /// Rejects plans without multipart versions. Replaces prior decoding options
    /// and invalidates cached prepared selections.
    pub fn with_busboy_text(
        mut self,
        options: crate::http::BusboyTextOptions,
    ) -> Result<Self, String> {
        if self.variants.is_empty()
            || self.variants.iter().any(|variant| {
                variant
                    .route
                    .request
                    .body
                    .as_ref()
                    .is_none_or(|body| body.kind != BodyKind::Multipart)
            })
        {
            return Err("Busboy text decoding requires multipart versions".into());
        }
        self.multipart_text = Some(options);
        self.identity = Arc::new(());
        Ok(self)
    }

    /// Select a version with the same limits as a configured multipart extractor.
    /// # Errors
    /// Returns validation errors when parsing or every candidate schema fails.
    pub fn select_with_multipart_limits(
        &self,
        request: &FrameworkRequest,
        limits: MultipartLimits,
    ) -> Result<Option<(RouteContract, SelectedVersion)>, Vec<ContractValidationError>> {
        self.selection_outcome(request, limits).selection
    }

    fn selection_outcome(
        &self,
        request: &FrameworkRequest,
        limits: MultipartLimits,
    ) -> SelectionOutcome {
        let mut parsed = None;
        let body = if let Some(parser) = &self.typescript_wire_parser {
            parser.parse(&self.variants[0].route, request).map(|value| {
                parsed = value.map(VersionParsedBody);
                parsed
                    .as_ref()
                    .and_then(|value| value.0.value.as_deref().cloned())
            })
        } else {
            self.body_value(request, limits)
        };
        let selection = match body {
            Err(error) => Err(vec![error]),
            Ok(body) => {
                if let Some(parsers) = &self.request_parsers {
                    let selection = self.select_parsed_request(
                        request,
                        body.as_ref(),
                        parsed.as_ref(),
                        parsers,
                    );
                    if parsed.is_none() {
                        parsed = Some(VersionParsedBody(ParsedJsonBody::from_wire(
                            body.map(Arc::new),
                            None,
                        )));
                    }
                    selection
                } else {
                    self.select_body(request, body)
                }
            }
        };
        SelectionOutcome { selection, parsed }
    }

    fn select_body(&self, request: &FrameworkRequest, body_value: Option<Value>) -> Selection {
        if self.variants.is_empty() {
            return Ok(None);
        }
        let query = self
            .typescript_simple_query
            .then(|| crate::http::parse_typescript_simple_query(request.raw_query.as_deref()));
        let mut matched = Vec::new();
        let mut selected = None;
        for variant in &self.variants {
            let Ok(candidate) = variant.hooks.preprocess(body_value.as_ref()) else {
                continue;
            };
            let Some(fields) = variant
                .parse_fields(request, query.as_ref())
                .map_err(|error| vec![error])?
            else {
                continue;
            };
            if variant
                .matches_body(
                    request,
                    candidate.as_deref(),
                    self.typescript_wire_parser.is_some(),
                )
                .map_err(|error| vec![error])?
            {
                let Ok(candidate) = variant.hooks.transform(candidate) else {
                    continue;
                };
                matched.push(variant.name.clone());
                selected = Some((variant, candidate, fields));
            }
        }
        let (variant, candidate, fields) = selected.ok_or_else(|| {
            vec![ContractValidationError::new(
                ContractValidationLocation::Body,
                "request does not match any declared API version",
            )]
        })?;
        let body = match candidate {
            Some(Cow::Borrowed(_)) => body_value,
            Some(Cow::Owned(value)) => Some(value),
            None => None,
        };
        Ok(Some((
            variant.route.clone(),
            SelectedVersion {
                version: variant.name.clone(),
                matched_versions: matched,
                parsed_body: body.map(Arc::new),
                parsed_fields: Arc::new(fields),
            },
        )))
    }
    // Versions share a parser. Decode once, even when many schemas are tried.
    fn body_value(
        &self,
        request: &FrameworkRequest,
        limits: MultipartLimits,
    ) -> Result<Option<Value>, ContractValidationError> {
        let Some(body) = self
            .variants
            .first()
            .and_then(|variant| variant.route.request.body.as_ref())
        else {
            return Ok(None);
        };
        decode_body_value(
            body.kind,
            &self.parsers,
            request,
            &self.parser_hooks,
            limits,
            self.multipart_text.as_ref(),
        )
    }
}
/// Decode a finite request using the same bounded parser as version selection.
/// Original wire bytes are never modified. Call transport authorization first.
/// # Errors
/// Rejects invalid parser settings, encodings, malformed values and exceeded limits.
pub fn parse_request_body(
    kind: BodyKind,
    parsers: &forklaunch_core::contract::BodyParserOptions,
    request: &FrameworkRequest,
) -> Result<Option<Value>, ContractValidationError> {
    validate_parser_options(parsers).map_err(|_| invalid_body("invalid parser options"))?;
    decode_body_value(
        kind,
        parsers,
        request,
        &BodyParserHooks::default(),
        MultipartLimits::default(),
        None,
    )
}

fn decode_body_value(
    kind: BodyKind,
    parsers: &forklaunch_core::contract::BodyParserOptions,
    request: &FrameworkRequest,
    hooks: &BodyParserHooks,
    limits: MultipartLimits,
    multipart_text: Option<&crate::http::BusboyTextOptions>,
) -> Result<Option<Value>, ContractValidationError> {
    let mut value =
        decode_body_value_in_wire_order(kind, parsers, request, hooks, limits, multipart_text)?;
    if let Some(value) = &mut value {
        json_request::javascript_member_order(value);
    }
    Ok(value)
}

fn decode_body_value_in_wire_order(
    kind: BodyKind,
    parsers: &forklaunch_core::contract::BodyParserOptions,
    request: &FrameworkRequest,
    hooks: &BodyParserHooks,
    limits: MultipartLimits,
    multipart_text: Option<&crate::http::BusboyTextOptions>,
) -> Result<Option<Value>, ContractValidationError> {
    let bytes = match &request.body {
        Body::Empty => &[][..],
        Body::Text(text) => text.as_bytes(),
        Body::Binary(bytes) => bytes.as_slice(),
    };
    let default_charset = match kind {
        BodyKind::Json | BodyKind::Unknown => &parsers.json_default_charset,
        BodyKind::UrlEncoded => &parsers.form_default_charset,
        _ => &parsers.text_default_charset,
    };
    let charset = charset::charset(
        crate::http::header_value(&request.headers, "content-type"),
        default_charset,
    );
    let wire_options = match kind {
        BodyKind::Json | BodyKind::Unknown => Some(&parsers.json_wire),
        BodyKind::Text => Some(&parsers.text_wire),
        BodyKind::UrlEncoded => Some(&parsers.form_wire),
        BodyKind::Binary | BodyKind::File => Some(&parsers.raw_wire),
        _ => None,
    };
    if matches!(
        kind,
        BodyKind::Json | BodyKind::Unknown | BodyKind::Text | BodyKind::UrlEncoded
    ) {
        if (matches!(kind, BodyKind::Json | BodyKind::Unknown) && !charset.starts_with("utf-"))
            || (kind == BodyKind::UrlEncoded && charset != "utf-8" && charset != "iso-8859-1")
        {
            return Err(charset::unsupported());
        }
        charset::decode(&[], &charset)?;
    }
    let decoded = if let Some(options) = wire_options {
        wire::decode(
            bytes,
            crate::http::header_value(&request.headers, "content-encoding"),
            options,
        )?
    } else {
        std::borrow::Cow::Borrowed(bytes)
    };
    let bytes = decoded.as_ref();
    if wire_options.is_some() {
        let encoding = if matches!(kind, BodyKind::File | BodyKind::Binary) {
            None
        } else {
            Some(charset.as_str())
        };
        hooks.verify(request, bytes, encoding)?;
    }
    let value = match kind {
        BodyKind::Json | BodyKind::Unknown => {
            return parse_json_body(bytes, &charset, parsers.json_strict, hooks);
        }
        BodyKind::File | BodyKind::Binary => {
            Value::Array(bytes.iter().copied().map(Value::from).collect())
        }
        BodyKind::UrlEncoded => form::parse_decoded(
            &charset::decode(bytes, &charset)?,
            charset == "iso-8859-1",
            parsers,
        )?,
        BodyKind::Multipart => multipart::parse_with_text(
            bytes,
            crate::http::header_value(&request.headers, "content-type")
                .ok_or_else(|| invalid_body("missing multipart content type"))?,
            limits,
            multipart_text,
        )?,
        _ => Value::String(charset::decode(bytes, &charset)?.into_owned()),
    };
    Ok(Some(value))
}

fn parse_json_body(
    bytes: &[u8],
    charset: &str,
    strict: bool,
    hooks: &BodyParserHooks,
) -> Result<Option<Value>, ContractValidationError> {
    let text = charset::decode(bytes, charset)?;
    if text.is_empty() {
        return Ok(Some(serde_json::json!({})));
    }
    let first = text
        .trim_start_matches([' ', '\t', '\r', '\n'])
        .as_bytes()
        .first();
    if strict && !matches!(first, Some(b'{' | b'[')) {
        return Err(invalid_body("JSON body must be an object or array"));
    }
    if hooks.has_json_reviver() {
        return hooks.revive_json(&text);
    }
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|_| invalid_body("invalid JSON body"))
}

/// Check bounded parser settings before mounting a transport.
/// # Errors
/// Rejects unsupported charsets and unbounded parser limits.
pub fn validate_parser_options(
    parsers: &forklaunch_core::contract::BodyParserOptions,
) -> Result<(), String> {
    for options in [
        &parsers.json_wire,
        &parsers.text_wire,
        &parsers.form_wire,
        &parsers.raw_wire,
    ] {
        if options.limit_bytes > 32 * 1024 * 1024 {
            return Err("Parser byte limit exceeds 32 MiB".into());
        }
    }

    if parsers.form_parameter_limit == 0
        || parsers.form_parameter_limit > 100_000
        || parsers.form_depth > 32
    {
        return Err("Invalid form parser limits (depth <= 32, parameters 1..=100000)".into());
    }
    if !parsers.json_default_charset.starts_with("utf-")
        || !matches!(
            parsers.form_default_charset.as_str(),
            "utf-8" | "iso-8859-1"
        )
    {
        return Err("Invalid default parser charset".into());
    }
    for charset in [
        &parsers.json_default_charset,
        &parsers.text_default_charset,
        &parsers.form_default_charset,
    ] {
        charset::decode(&[], charset).map_err(|_| "Unsupported default parser charset")?;
    }
    Ok(())
}

// body-parser's UTF-8 decoder replaces malformed sequences and strips one BOM.
fn decode_utf8(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))
}
/// Decode the bounded portable multipart profile used by generated SDK version matching.
/// Rejects duplicate/non-ASCII names and nonportable charset/header/file conventions.
/// # Errors
/// Rejects malformed bodies, invalid UTF-8 text and exceeded part/byte limits.
pub fn decode_multipart_value(
    bytes: &[u8],
    content_type: &str,
) -> Result<Value, ContractValidationError> {
    if bytes.len() > MultipartLimits::default().total_bytes {
        return Err(body_too_large("multipart body exceeds byte limit"));
    }
    multipart::parse_portable(bytes, content_type)
}
pub(super) fn parse_form_value(
    bytes: &[u8],
    content_type: &str,
) -> Result<Value, ContractValidationError> {
    parse_form_value_with_options(
        bytes,
        content_type,
        &forklaunch_core::contract::BodyParserOptions::default(),
    )
}
pub(super) fn parse_form_value_with_options(
    bytes: &[u8],
    content_type: &str,
    options: &forklaunch_core::contract::BodyParserOptions,
) -> Result<Value, ContractValidationError> {
    validate_parser_options(options).map_err(|_| invalid_body("invalid form parser options"))?;
    if bytes.len() > options.form_wire.limit_bytes {
        return Err(body_too_large("form exceeds byte limit"));
    }
    let charset = charset::charset(Some(content_type), &options.form_default_charset);
    if !matches!(charset.as_str(), "utf-8" | "iso-8859-1") {
        return Err(charset::unsupported());
    }
    form::parse_decoded(
        &charset::decode(bytes, &charset)?,
        charset == "iso-8859-1",
        options,
    )
}

fn invalid_body(message: &str) -> ContractValidationError {
    ContractValidationError::new(ContractValidationLocation::BodyParser, message)
}
fn body_too_large(message: &str) -> ContractValidationError {
    ContractValidationError::new(ContractValidationLocation::BodySize, message)
}
fn parser_kind(body: Option<&forklaunch_core::contract::BodyContract>) -> Option<BodyKind> {
    body.map(|body| match body.kind {
        BodyKind::File => BodyKind::Binary,
        BodyKind::Unknown => BodyKind::Json,
        kind => kind,
    })
}
fn decode_schema(schema: &SchemaRef, remaining_bytes: &mut usize) -> Result<Option<Value>, String> {
    schema
        .json_schema
        .as_deref()
        .map(|encoded| {
            *remaining_bytes = remaining_bytes
                .checked_sub(encoded.len())
                .ok_or("Version schemas exceed 32 MiB")?;
            serde_json::from_str(encoded).map_err(|_| "invalid version schema JSON".to_owned())
        })
        .transpose()
}
fn compile_schema(
    schema: &SchemaRef,
    remaining_bytes: &mut usize,
) -> Result<Option<Arc<CheckedJsonValidator>>, String> {
    decode_schema(schema, remaining_bytes)?
        .as_ref()
        .map(schema_validation::compile_checked)
        .transpose()
}
fn compile_group(
    schema: Option<&SchemaRef>,
    remaining: &mut usize,
) -> Result<Option<Arc<CheckedJsonValidator>>, String> {
    schema
        .map(|schema| {
            let value = decode_schema(schema, remaining)?.ok_or("missing request group schema")?;
            if value.get("type").and_then(Value::as_str) != Some("object") {
                return Err("request group schema must declare an object".into());
            }
            schema_validation::compile_checked(&value)
        })
        .transpose()
}
fn compile_body(
    body: &forklaunch_core::contract::BodyContract,
    remaining_bytes: &mut usize,
) -> Result<Option<BodyValidator>, String> {
    let Some(schema) = decode_schema(&body.schema, remaining_bytes)? else {
        return Ok(None);
    };
    let validator = if body.kind == BodyKind::Multipart {
        schema_validation::compile_multipart_checked(&schema)?
    } else {
        schema_validation::compile_checked(&schema)?
    };
    if !matches!(body.kind, BodyKind::File | BodyKind::Binary) {
        return Ok(Some(BodyValidator::Value(validator)));
    }
    if schema.is_boolean() || schema.get("type").and_then(Value::as_str) == Some("array") {
        return Ok(Some(BodyValidator::Bytes(validator)));
    }
    // OpenAPI's binary string describes opaque wire bytes, not UTF-8 text.
    // Do not silently discard assertions that cannot apply to that representation.
    if schema_validation::opaque_binary_schema(&schema) {
        Ok(Some(BodyValidator::Opaque))
    } else {
        Err("Versioned binary/file schemas require an opaque binary string, a byte-array schema, or a boolean schema".into())
    }
}

impl Variant {
    fn matches_body(
        &self,
        request: &FrameworkRequest,
        body_value: Option<&Value>,
        parsed_wire: bool,
    ) -> Result<bool, ContractValidationError> {
        if !parsed_wire
            && !validate_request_body(&self.route.request, request, body_value.is_some()).is_empty()
        {
            return Ok(false);
        }
        if body_value.is_none() && self.route.request.body_optional {
            return Ok(true);
        }
        match &self.body {
            None | Some(BodyValidator::Opaque) => Ok(true),
            Some(BodyValidator::Value(validate) | BodyValidator::Bytes(validate)) => {
                match body_value.map(|value| validate(value)) {
                    None | Some(Err(SchemaValidationFailure::Invalid(_))) => Ok(false),
                    Some(Ok(())) => Ok(true),
                    Some(Err(SchemaValidationFailure::Operational)) => {
                        Err(schema_execution_error())
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{FrameworkResponse, HttpMethod, LocalRouter, RequestContext};
    use forklaunch_core::contract::{
        BodyContract, FieldContract, RequestContract, ResponseContract, VersionedContract,
    };
    use serde_json::json;

    fn contract() -> RouteContract {
        let mut route = RouteContract::post("Versioned", "Versioned", "/items/:id");
        route
            .request
            .params
            .push(FieldContract::required("id", "Id"));
        for (name, minimum, status) in [("z-first", 0, 201), ("a-last", 10, 202)] {
            let mut body = BodyContract::json("Input");
            body.schema = body.schema.with_json_schema(json!({"type":"object","required":["value"],"properties":{"value":{"type":"integer","minimum":minimum}},"additionalProperties":false}).to_string());
            route = route.with_version(VersionedContract {
                version: name.into(),
                request: RequestContract::default().with_body(body),
                responses: vec![ResponseContract::new(status, None)],
            });
        }
        route
    }
    fn request(body: &str) -> FrameworkRequest {
        let mut request = FrameworkRequest::new(
            HttpMethod::Post,
            "/items/one",
            RequestContext::new("version-test"),
        )
        .with_header("content-type", "application/json")
        .with_body(Body::Text(body.into()));
        request.params.insert("id".into(), "one".into());
        request
    }
    #[test]
    fn invalid_parser_configuration_rejects_registration() {
        for options in [
            forklaunch_core::contract::BodyParserOptions {
                json_wire: forklaunch_core::contract::BodyParserWireOptions {
                    limit_bytes: 32 * 1024 * 1024 + 1,
                    ..Default::default()
                },
                ..Default::default()
            },
            forklaunch_core::contract::BodyParserOptions {
                form_parameter_limit: 0,
                ..Default::default()
            },
            forklaunch_core::contract::BodyParserOptions {
                form_parameter_limit: 100_001,
                ..Default::default()
            },
            forklaunch_core::contract::BodyParserOptions {
                form_depth: 33,
                ..Default::default()
            },
            forklaunch_core::contract::BodyParserOptions {
                json_default_charset: "latin1".into(),
                ..Default::default()
            },
            forklaunch_core::contract::BodyParserOptions {
                form_default_charset: "utf-16le".into(),
                ..Default::default()
            },
            forklaunch_core::contract::BodyParserOptions {
                text_default_charset: "unknown".into(),
                ..Default::default()
            },
        ] {
            let mut route = contract();
            route.options.body_parser = options;
            assert!(VersionedRoutePlan::compile(&route).is_err());
        }
    }

    #[test]
    fn local_handlers_receive_parsed_values_and_original_bytes() {
        let mut route = contract();
        for version in route.versions.values_mut() {
            let body = version.request.body.as_mut().unwrap();
            body.kind = BodyKind::Text;
            body.content_type = Some("text/plain".into());
            body.schema.json_schema = Some(r#"{"type":"string"}"#.into());
        }
        let mut router = LocalRouter::new();
        router.register(route, |request: FrameworkRequest| {
            assert_eq!(
                request.version.unwrap().parsed_body.as_deref(),
                Some(&json!("�"))
            );
            assert_eq!(request.body, Body::Binary(vec![0xef, 0xbb, 0xbf, 255]));
            Ok(FrameworkResponse::new(202))
        });
        let request = FrameworkRequest::new(
            HttpMethod::Post,
            "/items/one",
            RequestContext::new("parsed"),
        )
        .with_header("content-type", "text/plain")
        .with_body(Body::Binary(vec![0xef, 0xbb, 0xbf, 255]));
        assert_eq!(router.execute(request).unwrap().status_code, 202);
    }

    #[test]
    fn json_parser_failures_are_fatal_even_for_unconstrained_schemas() {
        let mut route = contract();
        for version in route.versions.values_mut() {
            version.request.body.as_mut().unwrap().schema.json_schema = Some("true".into());
        }
        let plan = VersionedRoutePlan::compile(&route).unwrap();
        for input in ["null", "42", "true", "\"string\"", " ", "{"] {
            let errors = plan.select(&request(input)).unwrap_err();
            assert!(errors.iter().all(ContractValidationError::is_parser_error));
        }
        for input in ["", "{}", "[]", "\u{feff}{}"] {
            let wire = request(input);
            assert!(plan.select(&wire).unwrap().is_some());
            assert_eq!(wire.body, Body::Text(input.into()));
        }
    }

    #[test]
    fn text_versions_normalize_empty_local_bodies_without_altering_bytes() {
        let mut route = contract();
        for version in route.versions.values_mut() {
            let body = version.request.body.as_mut().unwrap();
            body.kind = BodyKind::Text;
            body.content_type = Some("text/plain".into());
            body.schema.json_schema = Some(r#"{"type":"string"}"#.into());
        }
        let mut router = LocalRouter::new();
        router.register(route, |request: FrameworkRequest| {
            assert_eq!(request.version.unwrap().version, "a-last");
            assert_eq!(request.body, Body::Text(String::new()));
            Ok(FrameworkResponse::new(202))
        });
        let request =
            FrameworkRequest::new(HttpMethod::Post, "/items/one", RequestContext::new("empty"))
                .with_header("content-type", "text/plain");
        assert_eq!(router.execute(request).unwrap().status_code, 202);
    }

    #[test]
    fn last_matching_declaration_selects_response_contract_and_all_matches() {
        let route = contract();
        let plan = VersionedRoutePlan::compile(&route).unwrap();
        for (body, name, matches, status) in [
            (r#"{"value":1}"#, "z-first", 1, 201),
            (r#"{"value":11}"#, "a-last", 2, 202),
        ] {
            let (selected, version) = plan.select(&request(body)).unwrap().unwrap();
            assert_eq!(version.version, name);
            assert_eq!(version.matched_versions.len(), matches);
            assert_eq!(selected.responses[0].status_code, status);
            assert_eq!(selected.request.params, route.request.params);
        }
        for invalid in [
            r#"{"value":-1}"#,
            r#"{"value":"10"}"#,
            r#"{"value":11,"extra":true}"#,
            "not json",
        ] {
            assert!(plan.select(&request(invalid)).is_err());
        }
        let mut missing = request(r#"{"value":1}"#);
        missing.params.clear();
        assert!(plan.select(&missing).is_err());
    }
    #[test]
    fn local_dispatch_exposes_selection_and_checks_selected_response_status() {
        let mut router = LocalRouter::new();
        router.register(contract(), |request: FrameworkRequest| {
            let version = request.version.unwrap();
            Ok(FrameworkResponse::new(if version.version == "a-last" {
                202
            } else {
                201
            }))
        });
        assert_eq!(
            router
                .execute(request(r#"{"value":1}"#))
                .unwrap()
                .status_code,
            201
        );
        assert_eq!(
            router
                .execute(request(r#"{"value":11}"#))
                .unwrap()
                .status_code,
            202
        );
        assert_eq!(
            router
                .execute(request(r#"{"value":-1}"#))
                .unwrap()
                .status_code,
            400
        );
        let mut invalid = LocalRouter::new();
        invalid.register(contract(), |_: FrameworkRequest| {
            Ok(FrameworkResponse::new(201))
        });
        assert_eq!(
            invalid
                .execute(request(r#"{"value":11}"#))
                .unwrap()
                .status_code,
            500
        );
    }
    #[test]
    fn binary_versions_validate_bytes_and_reject_unsupported_string_assertions() {
        let mut route = contract();
        for version in route.versions.values_mut() {
            let body = version.request.body.as_mut().unwrap();
            body.kind = BodyKind::Binary;
            body.content_type = Some("application/octet-stream".into());
            body.schema.json_schema = Some(json!({"type":"array","items":{"type":"integer","minimum":0,"maximum":255},"minItems":if version.version=="a-last" {2} else {0}}).to_string());
        }
        let plan = VersionedRoutePlan::compile(&route).unwrap();
        for (bytes, expected) in [
            (vec![], "z-first"),
            (vec![255], "z-first"),
            (vec![0, 255], "a-last"),
        ] {
            let input = request("")
                .with_header("content-type", "application/octet-stream")
                .with_body(Body::Binary(bytes));
            assert_eq!(plan.select(&input).unwrap().unwrap().1.version, expected);
        }
        let body = route.versions["a-last"].request.body.as_mut().unwrap();
        body.schema.json_schema =
            Some(json!({"type":"string","format":"binary","minLength":1}).to_string());
        assert!(
            VersionedRoutePlan::compile(&route)
                .unwrap_err()
                .contains("opaque binary string")
        );
        for schema in [
            json!({"type":"string","format":"binary","title":"File"}),
            json!(true),
            json!(false),
        ] {
            route.versions["a-last"]
                .request
                .body
                .as_mut()
                .unwrap()
                .schema
                .json_schema = Some(schema.to_string());
            let plan = VersionedRoutePlan::compile(&route).unwrap();
            let input = request("")
                .with_header("content-type", "application/octet-stream")
                .with_body(Body::Binary(vec![255]));
            assert_eq!(
                plan.select(&input).unwrap().unwrap().1.version,
                if schema == json!(false) {
                    "z-first"
                } else {
                    "a-last"
                }
            );
        }
    }
    #[test]
    fn parser_failures_are_fatal_even_without_embedded_schemas_or_validation() {
        use forklaunch_core::contract::ValidationMode;
        for mode in [
            ValidationMode::Error,
            ValidationMode::Warning,
            ValidationMode::None,
        ] {
            for (kind, mime, wire, status) in [
                (BodyKind::Json, "application/json", "{".into(), 400),
                (
                    BodyKind::UrlEncoded,
                    "application/x-www-form-urlencoded",
                    "a=x&".repeat(1000),
                    413,
                ),
                (
                    BodyKind::UrlEncoded,
                    "application/x-www-form-urlencoded",
                    format!("a{}=x", "[b]".repeat(33)),
                    400,
                ),
            ] {
                let mut route = contract();
                route.options.request_validation = mode;
                for version in route.versions.values_mut() {
                    let body = version.request.body.as_mut().unwrap();
                    body.kind = kind;
                    body.content_type = Some(mime.into());
                    body.schema.json_schema = None;
                }
                let mut router = LocalRouter::new();
                router.register(
                    route,
                    |_: FrameworkRequest| -> forklaunch_common::FrameworkResult<FrameworkResponse> {
                        panic!("parser failure entered handler")
                    },
                );
                let response = router
                    .execute(request(&wire).with_header("content-type", mime))
                    .unwrap();
                assert_eq!(response.status_code, status);
                assert!(response.body.as_text().unwrap().contains("version-test"));
            }
        }
    }
    #[test]
    fn malformed_and_unbundled_version_schemas_fail_during_construction() {
        for schema in [
            r#"{"$ref":"https://unbundled.invalid/schema"}"#,
            r#"{"minimum":"bad"}"#,
        ] {
            let mut route = contract();
            route.versions["z-first"]
                .request
                .body
                .as_mut()
                .unwrap()
                .schema
                .json_schema = Some(schema.into());
            assert!(VersionedRoutePlan::compile(&route).is_err());
        }
    }
}

#[cfg(test)]
#[path = "versioning_simple_query_tests.rs"]
mod simple_query_tests;

#[cfg(test)]
#[path = "versioning_json_wire_tests.rs"]
mod json_wire_tests;

#[cfg(test)]
#[path = "versioning_text_wire_tests.rs"]
mod text_wire_tests;

fn schema_execution_error() -> ContractValidationError {
    ContractValidationError::new(
        ContractValidationLocation::SchemaExecution,
        "Schema validation could not be completed",
    )
}

#[cfg(all(test, feature = "zod-regex"))]
mod regex_tests {
    use super::*;
    use crate::http::{FrameworkResponse, HttpMethod, LocalRouter, RequestContext};
    use forklaunch_core::contract::{
        BodyContract, FieldContract, RequestContract, ResponseContract, ValidationMode,
        VersionedContract,
    };
    use serde_json::json;
    #[test]
    fn schema_execution_failure_is_fatal_in_all_modes_and_versions() {
        let schema=json!({"type":"string","x-forklaunch-zod-regex":{"version":1,"rules":[{"sourceUnits":"^(a+)+$".encode_utf16().collect::<Vec<_>>(),"flags":""}]}}).to_string();
        for mode in [
            ValidationMode::None,
            ValidationMode::Warning,
            ValidationMode::Error,
        ] {
            for field in [false, true] {
                let mut request = RequestContract::default();
                if field {
                    let mut query = FieldContract::required("value", "Value");
                    query.schema = query.schema.with_json_schema(&schema);
                    request.query.push(query);
                } else {
                    let mut body = BodyContract::json("Input");
                    body.schema=body.schema.with_json_schema(json!({"type":"object","properties":{"value":serde_json::from_str::<Value>(&schema).unwrap()},"required":["value"]}).to_string());
                    request = request.with_body(body);
                }
                let fallback = if field {
                    RequestContract::default()
                } else {
                    RequestContract::default().with_body(BodyContract::json("Fallback"))
                };
                let mut route = RouteContract::post("Regex", "Regex", "/regex")
                    .with_version(VersionedContract {
                        version: "first".into(),
                        request,
                        responses: vec![ResponseContract::new(200, None)],
                    })
                    .with_version(VersionedContract {
                        version: "fallback".into(),
                        request: fallback,
                        responses: vec![ResponseContract::new(200, None)],
                    });
                route.options.request_validation = mode;
                let mut router = LocalRouter::new();
                router.register(
                    route,
                    |_: FrameworkRequest| -> forklaunch_common::FrameworkResult<FrameworkResponse> {
                        panic!("operational failure entered handler")
                    },
                );
                let value = format!("{}!", "a".repeat(40));
                let mut incoming = FrameworkRequest::new(
                    HttpMethod::Post,
                    "/regex",
                    RequestContext::new("regex-correlation"),
                );
                if field {
                    incoming.query.insert("value".into(), value);
                } else {
                    incoming = incoming
                        .with_header("content-type", "application/json")
                        .with_body(Body::Text(json!({"value":value}).to_string()));
                }
                let response = router.execute(incoming).unwrap();
                assert_eq!(response.status_code, 500);
                let body = response.body.as_text().unwrap();
                assert!(body.contains("regex-correlation"));
                assert!(!body.contains(&"a".repeat(40)));
            }
        }
    }
}
