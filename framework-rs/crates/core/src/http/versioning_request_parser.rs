//! Trusted whole-request parsers: atomic projections and public ordered diagnostics.
use super::{
    SelectedVersion, Selection, VersionFieldValues, VersionedRoutePlan, schema_execution_error,
};
use crate::http::{
    ContractValidationError, ContractValidationLocation, FrameworkRequest, ParseError,
    pretty_print_parse_errors,
};
use forklaunch_core::contract::RouteContract;
use serde_json::{Map, Value};
use std::{
    any::Any,
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

/// Borrowed candidate input after the shared wire parser, before authentication
/// when the route opts into pre-auth selection. Header credentials stay on the
/// original request and cannot be rewritten through this projection.
pub struct VersionRequestInput<'a> {
    pub version: &'a str,
    pub fields: &'a VersionFieldValues,
    pub body: Option<&'a Value>,
    /// Internally bound, unmodified JSON parser source; unavailable after revivers.
    pub json_body: Option<VersionRequestJson<'a>>,
}
/// A trusted per-selection cache, never populated from request extensions.
/// A producer is called once across version candidates. Incompatible cached
/// types or producer failure remain operational; callbacks are trusted code.
#[derive(Clone, Copy)]
pub struct VersionRequestJson<'a> {
    source: &'a [u8],
    charset: &'a str,
    cache: &'a OnceLock<Result<Box<dyn Any + Send + Sync>, VersionRequestJsonError>>,
}
/// Failures preparing trusted source provenance, never schema validation errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionRequestJsonError {
    Preparation,
    TypeConflict,
}
impl VersionRequestJson<'_> {
    /// Prepare typed JSON provenance once from the built-in decoded source.
    /// Source bytes and decoded UTF-8 are independently limited to 16 MiB before
    /// the producer runs. This is a conservative additional profile limit.
    /// # Errors
    /// Producer failure or a conflicting producer type is operational.
    pub fn cached<T: Any + Send + Sync>(
        &self,
        producer: impl FnOnce(&str) -> Result<T, VersionRequestJsonError>,
    ) -> Result<&T, VersionRequestJsonError> {
        self.cache
            .get_or_init(|| {
                if self.source.len() > 16 * 1024 * 1024 {
                    return Err(VersionRequestJsonError::Preparation);
                }
                let source = super::charset::decode(self.source, self.charset)
                    .map_err(|_| VersionRequestJsonError::Preparation)?;
                if source.len() > 16 * 1024 * 1024 {
                    return Err(VersionRequestJsonError::Preparation);
                }
                producer(&source).map(|value| Box::new(value) as Box<dyn Any + Send + Sync>)
            })
            .as_ref()
            .map_err(|error| *error)?
            .downcast_ref::<T>()
            .ok_or(VersionRequestJsonError::TypeConflict)
    }
}
impl VersionRequestInput<'_> {
    /// Materialize the finite JSON request shape for an application evaluator.
    /// Absence omits `body`; JSON null remains present. This is not an ordered
    /// raw-wire object and does not reconstruct query repetitions without the
    /// explicit simple-query parser profile. The caller owns this allocation.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let Value::Object(mut result) = self.fields_json() else {
            unreachable!()
        };
        if let Some(body) = self.body {
            result.insert("body".into(), body.clone());
        }
        Value::Object(result)
    }
    /// Materialize only field sections without cloning the parsed body.
    #[must_use]
    pub fn fields_json(&self) -> Value {
        let mut result = Map::new();
        for (name, fields) in [
            ("params", &self.fields.params),
            ("headers", &self.fields.headers),
            ("query", &self.fields.query),
        ] {
            result.insert(
                name.into(),
                Value::Object(
                    fields
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect(),
                ),
            );
        }
        Value::Object(result)
    }
}

/// Atomic application-schema output; raw request maps and bytes remain unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRequestProjection {
    pub fields: VersionFieldValues,
    pub body: Option<Value>,
}
impl VersionRequestProjection {
    /// Consume an evaluator's request object. Its params/query/headers must be
    /// objects; body absence and explicit null are retained independently.
    /// # Errors
    /// Invalid output shape is operational, not a client schema rejection.
    pub fn from_json(value: Value) -> Result<Self, VersionRequestParseFailure> {
        let Value::Object(mut object) = value else {
            return Err(VersionRequestParseFailure::Operational);
        };
        let mut section = |name| match object.remove(name) {
            Some(Value::Object(value)) => Ok(value.into_iter().collect()),
            _ => Err(VersionRequestParseFailure::Operational),
        };
        let fields = VersionFieldValues {
            params: section("params")?,
            headers: section("headers")?,
            query: section("query")?,
        };
        Ok(Self {
            fields,
            body: object.remove("body"),
        })
    }
}

/// Only `Validation` messages are declared public. Operational details are never
/// accepted as a string and remain fatal under Error, Warning and None modes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionRequestParseFailure {
    Validation(Vec<ParseError>),
    Operational,
}
type Parse = dyn for<'a> Fn(
        VersionRequestInput<'a>,
    ) -> Result<VersionRequestProjection, VersionRequestParseFailure>
    + Send
    + Sync;
/// A trusted schema-bound parser, evaluated once for each version in declaration
/// order. It owns schema semantics; generic JSON Schema errors are not relabeled
/// as Zod diagnostics. Register every version and do not combine with body/field hooks.
#[derive(Clone)]
pub struct VersionRequestParser(Arc<Parse>);
impl std::fmt::Debug for VersionRequestParser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VersionRequestParser")
            .finish_non_exhaustive()
    }
}
impl VersionRequestParser {
    #[must_use]
    pub fn new(
        callback: impl for<'a> Fn(
            VersionRequestInput<'a>,
        )
            -> Result<VersionRequestProjection, VersionRequestParseFailure>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self(Arc::new(callback))
    }
}

#[derive(Clone, Debug)]
pub(super) struct RegisteredRequestParsers {
    pub route: RouteContract,
    pub parsers: BTreeMap<String, VersionRequestParser>,
}

impl VersionedRoutePlan {
    /// Replace generic per-version schema interpretation with trusted whole-request
    /// parsers. Shared wire parsing and authentication/authorization are retained.
    /// Each version must be registered, including ones expected to reject input.
    /// Retained diagnostics have a conservative 1 MiB aggregate byte allowance,
    /// at most 10,000 errors per version and 128 path segments per error. Exceeding
    /// these limits is operational, even if another evaluated version succeeded.
    /// # Errors
    /// Rejects unknown/missing versions, unversioned routes and body/field hook conflicts.
    pub fn with_request_parsers(
        mut self,
        parsers: BTreeMap<String, VersionRequestParser>,
    ) -> Result<Self, String> {
        if self.variants.is_empty()
            || parsers.len() != self.variants.len()
            || self.variants.iter().any(|variant| {
                !parsers.contains_key(&variant.name)
                    || !variant.hooks.is_empty()
                    || variant.fields.iter().any(|field| !field.hooks.is_empty())
            })
        {
            return Err(
                "Whole-request parsers require every version and no body/field hooks".into(),
            );
        }
        self.request_parsers = Some(Arc::new(parsers));
        self.identity = Arc::new(());
        Ok(self)
    }
    pub(super) fn select_parsed_request(
        &self,
        request: &FrameworkRequest,
        body: Option<&Value>,
        parsed: Option<&super::VersionParsedBody>,
        parsers: &BTreeMap<String, VersionRequestParser>,
    ) -> Selection {
        let strings = |values: &BTreeMap<String, String>| {
            values
                .iter()
                .map(|(key, value)| (key.clone(), Value::String(value.clone())))
                .collect()
        };
        let fields = VersionFieldValues {
            params: strings(&request.params),
            headers: strings(&request.headers),
            query: if self.typescript_simple_query {
                crate::http::parse_typescript_simple_query(request.raw_query.as_deref())
            } else {
                strings(&request.query)
            },
        };
        let source = if matches!(
            self.typescript_wire_parser.as_ref(),
            Some(super::VersionWireParser::Json(_))
        ) && !self.parser_hooks.has_json_reviver()
        {
            parsed
                .and_then(super::VersionParsedBody::raw_body)
                .filter(|bytes| !bytes.is_empty())
        } else {
            None
        };
        let charset = super::charset::charset(
            crate::http::header_value(&request.headers, "content-type"),
            &self.parsers.json_default_charset,
        );
        let cache = OnceLock::new();
        let mut matched = Vec::new();
        let mut selected = None;
        let mut errors = Vec::new();
        let mut remaining = 1024 * 1024usize;
        for variant in &self.variants {
            match (parsers[&variant.name].0)(VersionRequestInput {
                version: &variant.name,
                fields: &fields,
                body,
                json_body: source.map(|source| VersionRequestJson {
                    source,
                    charset: &charset,
                    cache: &cache,
                }),
            }) {
                Ok(mut projection) => {
                    if !variant
                        .validate_groups(request, None, &projection.fields)
                        .map_err(|error| vec![error])?
                    {
                        continue;
                    }
                    projection.fields.headers =
                        projection.fields.project_wire_headers(&request.headers);
                    matched.push(variant.name.clone());
                    selected = Some((variant, projection));
                }
                Err(VersionRequestParseFailure::Validation(public)) => {
                    // Bound retained public reports before rendering or cloning
                    // path/prefix strings. Conservative overhead is intentional.
                    let Some(size) = diagnostic_size(&public, &variant.name) else {
                        return Err(vec![schema_execution_error()]);
                    };
                    let Some(next) = remaining.checked_sub(size) else {
                        return Err(vec![schema_execution_error()]);
                    };
                    remaining = next;
                    errors.push(ContractValidationError::new(
                        ContractValidationLocation::VersionRequestDiagnostic,
                        pretty_print_parse_errors(
                            Some(&public),
                            Some(&format!("Version {} request", variant.name)),
                        ),
                    ));
                }
                Err(VersionRequestParseFailure::Operational) => {
                    return Err(vec![schema_execution_error()]);
                }
            }
        }
        let Some((variant, projection)) = selected else {
            return Err(errors);
        };
        Ok(Some((
            variant.route.clone(),
            SelectedVersion {
                version: variant.name.clone(),
                matched_versions: matched,
                parsed_body: projection.body.map(Arc::new),
                parsed_fields: Arc::new(projection.fields),
            },
        )))
    }
}

fn diagnostic_size(errors: &[ParseError], version: &str) -> Option<usize> {
    if errors.len() > 10_000 {
        return None;
    }
    let mut size = version.len().checked_add(128)?;
    for error in errors {
        if error.path.len() > 128 {
            return None;
        }
        size = size.checked_add(error.message.len())?.checked_add(128)?;
        for segment in &error.path {
            size = size.checked_add(segment.len())?.checked_add(3)?;
        }
    }
    Some(size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{
        Body, HttpMethod, MultipartLimits, RequestContext, VersionBodyHooks,
        VersionBodyHooksRegistry,
    };
    use forklaunch_core::contract::{BodyContract, RequestContract, VersionedContract};
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[test]
    fn trusted_json_cache_is_single_producer_and_type_bound() {
        let cache = OnceLock::new();
        let source = VersionRequestJson {
            source: br#"{"z":1,"a":2}"#,
            charset: "utf-8",
            cache: &cache,
        };
        let calls = AtomicUsize::new(0);
        for _ in 0..3 {
            assert_eq!(
                source
                    .cached(|text| {
                        calls.fetch_add(1, Ordering::SeqCst);
                        Ok(text.to_owned())
                    })
                    .unwrap(),
                r#"{"z":1,"a":2}"#
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            source.cached::<usize>(|_| panic!("cached producer ran again")),
            Err(VersionRequestJsonError::TypeConflict)
        );
    }
    #[test]
    fn trusted_json_cache_decodes_charset_and_retains_failure() {
        let cache = OnceLock::new();
        let bytes = [b'{', 0, b'}', 0];
        let source = VersionRequestJson {
            source: &bytes,
            charset: "utf-16le",
            cache: &cache,
        };
        assert_eq!(source.cached(|text| Ok(text.to_owned())).unwrap(), "{}");
        let failure = OnceLock::new();
        let source = VersionRequestJson {
            source: b"{}",
            charset: "utf-8",
            cache: &failure,
        };
        assert_eq!(
            source.cached::<usize>(|_| Err(VersionRequestJsonError::Preparation)),
            Err(VersionRequestJsonError::Preparation)
        );
        assert_eq!(
            source.cached::<usize>(|_| panic!("failed producer ran again")),
            Err(VersionRequestJsonError::Preparation)
        );
        let oversized = vec![b' '; 16 * 1024 * 1024 + 1];
        let cache = OnceLock::new();
        let source = VersionRequestJson {
            source: &oversized,
            charset: "utf-8",
            cache: &cache,
        };
        assert_eq!(
            source.cached::<usize>(|_| panic!("over-limit source reached producer")),
            Err(VersionRequestJsonError::Preparation)
        );
    }
    fn route() -> RouteContract {
        let mut route = RouteContract::post("CheckedVersions", "Checked versions", "/checked");
        for name in ["first", "last"] {
            route = route.with_version(VersionedContract {
                version: name.into(),
                request: RequestContract::default().with_body(BodyContract::json("Input")),
                responses: vec![],
            });
        }
        route
    }
    fn request() -> FrameworkRequest {
        FrameworkRequest::new(
            HttpMethod::Post,
            "/checked",
            RequestContext::new("public-id"),
        )
        .with_body(Body::Text("{}".into()))
    }
    fn failure(calls: Arc<AtomicUsize>) -> VersionRequestParser {
        VersionRequestParser::new(move |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(VersionRequestParseFailure::Validation(vec![
                ParseError {
                    path: vec!["headers".into(), "tag".into()],
                    message: "header failure".into(),
                },
                ParseError {
                    path: vec!["body".into(), "flag".into()],
                    message: "body failure".into(),
                },
            ]))
        })
    }
    #[test]
    fn ordered_public_failures_are_cached_and_wire_bytes_preserved() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = VersionBodyHooksRegistry::default();
        let route = route();
        for name in ["first", "last"] {
            registry
                .insert_request_parser(&route, name, failure(calls.clone()))
                .unwrap();
        }
        let plan = registry.compile(&route).unwrap();
        let mut input = request();
        let prepared = plan.prepare_selection(&input, MultipartLimits::default());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let errors = plan
            .apply_prepared(&prepared, &mut input)
            .unwrap()
            .unwrap_err();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let response = crate::http::validation_error_response(&errors, "public-id");
        assert_eq!(response.status_code, 400);
        let Body::Text(text) = response.body else {
            panic!("expected text")
        };
        assert!(text.contains("Message: body failureVersion last request validation failed"));
        assert!(text.ends_with("\n\nCorrelation id: public-id"));
        assert_eq!(input.body, Body::Text("{}".into()));
        assert_eq!(input.version_parsed_body.unwrap().value(), Some(&json!({})));
        let changed = plan
            .clone()
            .with_request_parsers(
                [
                    ("first".into(), failure(calls.clone())),
                    ("last".into(), failure(calls)),
                ]
                .into_iter()
                .collect(),
            )
            .unwrap();
        assert!(changed.resolve_prepared(&prepared, &request()).is_err());
    }
    #[test]
    fn registration_rejects_missing_conflicting_and_stale_parsers() {
        let route = route();
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = VersionBodyHooksRegistry::default();
        registry
            .insert_request_parser(&route, "first", failure(calls.clone()))
            .unwrap();
        assert!(registry.compile(&route).is_err());
        assert!(
            registry
                .insert_request_parser(&route, "first", failure(calls.clone()))
                .is_err()
        );
        assert!(
            registry
                .insert(&route, "first", VersionBodyHooks::default())
                .is_err()
        );
        registry
            .insert_request_parser(&route, "last", failure(calls))
            .unwrap();
        let mut stale = route.clone();
        stale.options.request_validation = forklaunch_core::contract::ValidationMode::Warning;
        assert!(registry.compile(&stale).is_err());
        let plan = registry.compile(&route).unwrap();
        assert!(
            plan.with_body_hooks("first", VersionBodyHooks::default())
                .is_err()
        );
    }
    #[test]
    fn last_success_projects_atomically_and_operational_failure_dominates() {
        let success = VersionRequestParser::new(|input| {
            Ok(VersionRequestProjection {
                fields: input.fields.clone(),
                body: Some(json!({"defaulted":true})),
            })
        });
        let plan = VersionedRoutePlan::compile(&route())
            .unwrap()
            .with_request_parsers(
                [
                    ("first".into(), success.clone()),
                    ("last".into(), success.clone()),
                ]
                .into_iter()
                .collect(),
            )
            .unwrap();
        let (_, selected) = plan.select(&request()).unwrap().unwrap();
        assert_eq!(selected.version, "last");
        assert_eq!(selected.matched_versions, ["first", "last"]);
        assert_eq!(
            selected.parsed_body.as_deref(),
            Some(&json!({"defaulted":true}))
        );
        let operational =
            VersionRequestParser::new(|_| Err(VersionRequestParseFailure::Operational));
        let plan = plan
            .with_request_parsers(
                [("first".into(), success), ("last".into(), operational)]
                    .into_iter()
                    .collect(),
            )
            .unwrap();
        let errors = plan.select(&request()).unwrap_err();
        assert!(errors[0].is_parser_error());
        assert_eq!(
            crate::http::validation_error_response(&errors, "id").status_code,
            500
        );
        assert!(
            diagnostic_size(
                &[ParseError {
                    path: vec![String::new(); 129],
                    message: String::new()
                }],
                "v"
            )
            .is_none()
        );
    }
}
