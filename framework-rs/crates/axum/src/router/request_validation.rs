//! Application-supplied validators for declared unversioned request schemas.
use forklaunch_core::contract::{
    ContractRegistry, RequestContract, ResponseContract, RouteContract, ValidationMode,
};
use forklaunch_core::http::{ErrorKind, ErrorResponse, FrameworkRequest, FrameworkResponse};
use std::{collections::BTreeMap, sync::Arc};

type JsonTransform = dyn Fn(&FrameworkRequest) -> Result<Option<serde_json::Value>, JsonTransformFailure>
    + Send
    + Sync;

/// A trusted JSON schema callback distinguishes user validation failures from
/// evaluator/resource failures. Operational failures always produce a redacted
/// internal error, including when request validation is None or Warning.
#[derive(Debug, Clone)]
pub enum JsonTransformFailure {
    /// Public diagnostic response subject to the route's validation mode.
    Validation(FrameworkResponse),
    /// Internal evaluator failure; no diagnostic payload is exposed.
    Operational,
}

type RequestTransform = dyn Fn(
        &FrameworkRequest,
        Option<&str>,
    ) -> Result<forklaunch_core::http::RequestTransformOutput, FrameworkResponse>
    + Send
    + Sync;

#[derive(Clone)]
enum Callback {
    Reject(Arc<Validator>),
    JsonTransform(Arc<JsonTransform>),
    RequestTransform(Arc<RequestTransform>),
}

type Validator = dyn Fn(&FrameworkRequest, Option<&str>) -> Option<FrameworkResponse> + Send + Sync;

/// When a registered request callback executes. Native schema checks retain
/// their existing order; this controls only the explicitly registered callback.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RequestValidationStage {
    /// Preserve the native default: authenticate before callback validation.
    #[default]
    AfterAuthentication,
    /// Match TypeScript request-schema middleware ordering for this callback.
    /// Parsing runs first; contract authentication/authorization has not run yet.
    BeforeAuthentication,
}

#[derive(Clone)]
struct Registration {
    schema: RequestContract,
    stage: RequestValidationStage,
    rejection: Option<ResponseContract>,
    validate: Callback,
}

/// Validators are bound to a declared request schema and method/operation pair.
/// Rejection callbacks run after authentication by default and before local or Axum handlers,
/// only in error-validation mode. They supplement native contract checks.
/// The optional raw query preserves repeated keys that `FrameworkRequest::query`
/// flattens. A returned response rejects the request; its diagnostics must be safe
/// for public consumption. JSON transform registrations instead run before auth
/// in every mode. Handlers must consume `ParsedJsonBody` to observe transformed
/// values; raw Axum JSON/body extractors still consume original wire bytes.
/// This API does not translate validator semantics.
#[derive(Clone, Default)]
pub struct RequestValidators {
    entries: BTreeMap<String, Registration>,
}

impl RequestValidators {
    /// Register a validator for an unversioned declared request schema.
    /// # Errors
    /// Rejects versioned routes and duplicate method/operation registrations.
    pub fn insert(
        &mut self,
        route: &RouteContract,
        validate: impl Fn(&FrameworkRequest, Option<&str>) -> Option<FrameworkResponse>
        + Send
        + Sync
        + 'static,
    ) -> Result<(), String> {
        if !route.versions.is_empty() {
            return Err("Request validators require an unversioned route".into());
        }
        let key = key(route);
        if self.entries.contains_key(&key) {
            return Err("Duplicate request validator operation".into());
        }
        self.entries.insert(
            key,
            Registration {
                schema: route.request.clone(),
                stage: RequestValidationStage::default(),
                rejection: None,
                validate: Callback::Reject(Arc::new(validate)),
            },
        );
        Ok(())
    }

    /// Select execution order for an already registered callback. Only Error-mode
    /// callbacks run; None/Warning modes do not acquire new callback behavior.
    /// Parser failures precede both stages. Pre-auth diagnostics must be safe to
    /// expose without credentials; existing rejection metadata remains enforced.
    /// # Errors
    /// Rejects missing/stale registrations or moving a JSON transform after auth.
    pub fn set_stage(
        &mut self,
        route: &RouteContract,
        stage: RequestValidationStage,
    ) -> Result<(), String> {
        let entry = self
            .entries
            .get_mut(&key(route))
            .ok_or("Missing request validator registration")?;
        if !route.versions.is_empty() || entry.schema != route.request {
            return Err("Request validator schema no longer matches its route".into());
        }
        if matches!(
            entry.validate,
            Callback::JsonTransform(_) | Callback::RequestTransform(_)
        ) && stage != RequestValidationStage::BeforeAuthentication
        {
            return Err("Schema transforms must run before authentication".into());
        }
        entry.stage = stage;
        Ok(())
    }

    /// Register explicit rejection metadata alongside a validator. The declaration
    /// appears in the router registry, exported contracts, SDKs and `OpenAPI`.
    /// Runtime rejection status and media type must match; mismatches become a
    /// safe native 500 response. Body schema validation remains caller-owned.
    /// # Errors
    /// Rejects invalid error statuses, missing media types and conflicting route
    /// response declarations, in addition to normal registration constraints.
    pub fn insert_with_rejection(
        &mut self,
        route: &RouteContract,
        rejection: ResponseContract,
        validate: impl Fn(&FrameworkRequest, Option<&str>) -> Option<FrameworkResponse>
        + Send
        + Sync
        + 'static,
    ) -> Result<(), String> {
        if !(400..=599).contains(&rejection.status_code)
            || rejection.body.as_ref().is_some_and(|body| {
                body.content_type
                    .as_deref()
                    .is_none_or(|value| !valid_media_type(value))
            })
        {
            return Err(
                "Rejection declarations require an error status and explicit body media type"
                    .into(),
            );
        }
        check_conflict(route, &rejection)?;
        self.insert(route, validate)?;
        self.entries
            .get_mut(&key(route))
            .ok_or("Missing inserted validator")?
            .rejection = Some(rejection);
        Ok(())
    }

    /// Register a trusted JSON-body schema callback, executed once before auth
    /// in every validation mode. Input is immutable. Success atomically replaces
    /// `ParsedJsonBody::value`, including root removal, retaining original bytes.
    /// Error-mode failures return the declared public 400 response; None/Warning
    /// keep the entire original parsed value. Warning emits a static diagnostic
    /// without body data or callback payloads. Callback semantics are caller-owned;
    /// this does not implement generic Zod transforms or affect MCP invocation.
    /// # Errors
    /// Requires an exact JSON parser registration and an explicit 400 declaration.
    pub fn insert_json_transform(
        &mut self,
        route: &RouteContract,
        parsers: &forklaunch_core::http::JsonRequestParsers,
        rejection: ResponseContract,
        transform: impl Fn(&FrameworkRequest) -> Result<Option<serde_json::Value>, FrameworkResponse>
        + Send
        + Sync
        + 'static,
    ) -> Result<(), String> {
        self.insert_json_transform_checked(route, parsers, rejection, move |request| {
            transform(request).map_err(JsonTransformFailure::Validation)
        })
    }

    /// Register a JSON schema callback with explicit operational failures.
    /// Validation failures follow the configured Error/Warning/None policy;
    /// operational failures stop processing with a safe correlated 500 in every
    /// mode. Success preserves the same atomic projection and original bytes as
    /// [`Self::insert_json_transform`]. Parser and configured authentication order
    /// remain unchanged. This callback does not interpret schema definitions.
    /// # Errors
    /// Requires an exact JSON parser registration and a 400 rejection declaration.
    pub fn insert_json_transform_checked(
        &mut self,
        route: &RouteContract,
        parsers: &forklaunch_core::http::JsonRequestParsers,
        rejection: ResponseContract,
        transform: impl Fn(&FrameworkRequest) -> Result<Option<serde_json::Value>, JsonTransformFailure>
        + Send
        + Sync
        + 'static,
    ) -> Result<(), String> {
        parsers.validate_registration(route)?;
        if rejection.status_code != 400 {
            return Err("JSON schema transforms require a 400 rejection declaration".into());
        }
        self.insert_with_rejection(route, rejection, |_, _| None)?;
        let entry = self
            .entries
            .get_mut(&key(route))
            .ok_or("Missing request validator registration")?;
        entry.stage = RequestValidationStage::BeforeAuthentication;
        entry.validate = Callback::JsonTransform(Arc::new(transform));
        Ok(())
    }

    /// Register one trusted whole-request schema callback. All declared fields
    /// and an optional JSON body are committed atomically before auth in all modes.
    /// Failed None/Warning validation retains every original section; Error uses
    /// the declared public 400 response. A successful projection bypasses only
    /// native request-schema checks, never authentication or authorization.
    /// Headers are projected with TypeScript truthy/existing-key rules; native
    /// credential extraction and ordinary Axum extractors still use original wire.
    /// This does not implement generic Zod semantics, complex query parsing or MCP.
    /// # Errors
    /// Rejects duplicate/versioned registrations, non-400 rejection declarations,
    /// or declared bodies without an exact registered JSON parser.
    pub fn insert_request_transform(
        &mut self,
        route: &RouteContract,
        parsers: &forklaunch_core::http::JsonRequestParsers,
        rejection: ResponseContract,
        transform: impl Fn(
            &FrameworkRequest,
            Option<&str>,
        )
            -> Result<forklaunch_core::http::RequestTransformOutput, FrameworkResponse>
        + Send
        + Sync
        + 'static,
    ) -> Result<(), String> {
        if route.request.body.is_some() {
            parsers.validate_registration(route)?;
        }
        if rejection.status_code != 400 {
            return Err("Request transforms require a 400 declaration".into());
        }
        self.insert_with_rejection(route, rejection, |_, _| None)?;
        let entry = self
            .entries
            .get_mut(&key(route))
            .ok_or("Missing request validator registration")?;
        entry.stage = RequestValidationStage::BeforeAuthentication;
        entry.validate = Callback::RequestTransform(Arc::new(transform));
        Ok(())
    }

    pub(super) fn validate_json_parsers(
        &self,
        routes: &[RouteContract],
        parsers: &forklaunch_core::http::JsonRequestParsers,
    ) -> Result<(), String> {
        for route in routes {
            if self.entries.get(&key(route)).is_some_and(|entry| {
                matches!(entry.validate, Callback::JsonTransform(_))
                    || (matches!(entry.validate, Callback::RequestTransform(_))
                        && route.request.body.is_some())
            }) {
                parsers.validate_registration(route)?;
            }
        }
        Ok(())
    }

    pub(super) fn transform_request(
        &self,
        route: &RouteContract,
        request: &mut FrameworkRequest,
        raw_query: Option<&str>,
    ) -> Option<FrameworkResponse> {
        let entry = self.entries.get(&key(route))?;
        let result = match &entry.validate {
            Callback::Reject(_) => return None,
            Callback::JsonTransform(transform) => {
                let Some(parsed) = request.parsed_json.clone() else {
                    return Some(internal_failure(request));
                };
                transform(request).map(|value| {
                    request.parsed_json = Some(parsed.with_transformed_value(value));
                })
            }
            Callback::RequestTransform(transform) => {
                if route.request.body.is_some() && request.parsed_json.is_none() {
                    return Some(internal_failure(request));
                }
                match transform(request, raw_query) {
                    Ok(output) => {
                        if route.request.body.is_none() && output.body.is_some() {
                            return Some(internal_failure(request));
                        }
                        let fields = forklaunch_core::http::ParsedRequestFields::from_validated(
                            output.fields,
                            &request.headers,
                        );
                        if let Some(parsed) = request.parsed_json.take() {
                            request.parsed_json = Some(parsed.with_transformed_value(output.body));
                        }
                        request.parsed_fields = Some(fields);
                        Ok(())
                    }
                    Err(response) => Err(JsonTransformFailure::Validation(response)),
                }
            }
        };
        match result {
            Ok(()) => None,
            Err(JsonTransformFailure::Operational) => Some(internal_failure(request)),
            Err(JsonTransformFailure::Validation(response))
                if route.options.request_validation == ValidationMode::Error =>
            {
                Some(check_rejection(entry, request, response))
            }
            Err(_) => {
                if route.options.request_validation == ValidationMode::Warning {
                    tracing::warn!(operation = %route.operation_name, correlation_id = %request.context.correlation_id, "request schema validation failed; original request retained");
                }
                None
            }
        }
    }

    pub(super) fn project(
        &self,
        registry: &ContractRegistry,
    ) -> Result<Option<ContractRegistry>, String> {
        if !self.entries.values().any(|entry| entry.rejection.is_some()) {
            return Ok(None);
        }
        let mut projected = registry.clone();
        for route in registry.routes() {
            if let Some(rejection) = self
                .entries
                .get(&key(route))
                .and_then(|entry| entry.rejection.as_ref())
            {
                check_conflict(route, rejection)?;
                if route.options.request_validation == ValidationMode::Error {
                    projected.declare_response(
                        route.method,
                        &route.operation_name,
                        rejection.clone(),
                    )?;
                }
            }
        }
        Ok(Some(projected))
    }

    pub(super) fn contains(&self, route: &RouteContract) -> bool {
        self.entries.contains_key(&key(route))
    }

    pub(super) fn validate_routes(&self, routes: &[RouteContract]) -> Result<(), String> {
        if routes.iter().any(|route| {
            route.versions.is_empty()
                && (route.request.query_schema.is_some() || route.request.headers_schema.is_some())
        }) {
            return Err("whole query/header schemas require the native typed router or versioned request plan; legacy unversioned routing cannot discard group constraints".into());
        }
        for (key, registration) in &self.entries {
            let matches: Vec<_> = routes
                .iter()
                .filter(|route| self::key(route) == *key)
                .collect();
            if matches.len() != 1 {
                return Err("Request validator must identify exactly one route".into());
            }
            let route = matches[0];
            if !route.versions.is_empty() || route.request != registration.schema {
                return Err("Request validator schema no longer matches its route".into());
            }
            if let Some(rejection) = &registration.rejection {
                check_conflict(route, rejection)?;
            }
        }
        Ok(())
    }

    pub(super) fn extend(&mut self, other: Self) {
        for (key, entry) in other.entries {
            assert!(
                self.entries.insert(key, entry).is_none(),
                "duplicate request validator operation"
            );
        }
    }

    pub(super) fn reject_at(
        &self,
        stage: RequestValidationStage,
        route: &RouteContract,
        request: &FrameworkRequest,
        raw_query: Option<&str>,
    ) -> Option<FrameworkResponse> {
        if self.entries.get(&key(route))?.stage != stage {
            return None;
        }
        self.reject(route, request, raw_query)
    }

    pub(super) fn reject(
        &self,
        route: &RouteContract,
        request: &FrameworkRequest,
        raw_query: Option<&str>,
    ) -> Option<FrameworkResponse> {
        if route.options.request_validation != ValidationMode::Error {
            return None;
        }
        let entry = self.entries.get(&key(route))?;
        let Callback::Reject(validate) = &entry.validate else {
            return None;
        };
        let response = validate(request, raw_query)?;
        Some(check_rejection(entry, request, response))
    }
}

fn internal_failure(request: &FrameworkRequest) -> FrameworkResponse {
    ErrorResponse::new(ErrorKind::Internal, "internal server error")
        .with_correlation_id(request.context.correlation_id.clone())
        .to_response()
}
fn check_rejection(
    entry: &Registration,
    request: &FrameworkRequest,
    response: FrameworkResponse,
) -> FrameworkResponse {
    if entry
        .rejection
        .as_ref()
        .is_some_and(|declared| !matches_rejection(declared, &response))
    {
        return internal_failure(request);
    }
    response
}

fn check_conflict(route: &RouteContract, rejection: &ResponseContract) -> Result<(), String> {
    if route
        .responses
        .iter()
        .any(|response| response.status_code == rejection.status_code && response != rejection)
    {
        Err("Conflicting validator rejection response declaration".into())
    } else {
        Ok(())
    }
}

fn valid_media_type(value: &str) -> bool {
    let Some((kind, subtype)) = value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .split_once('/')
    else {
        return false;
    };
    let token = |value: &str| {
        !value.is_empty()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
    };
    token(kind) && token(subtype) && axum::http::HeaderValue::from_str(value).is_ok()
}

fn matches_rejection(declared: &ResponseContract, response: &FrameworkResponse) -> bool {
    if declared.status_code != response.status_code {
        return false;
    }
    let Some(body) = &declared.body else {
        return response.body.is_empty();
    };
    let actual = response
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        .map(|(_, value)| value.as_str());
    actual
        .zip(body.content_type.as_deref())
        .is_some_and(|(actual, expected)| {
            actual
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .eq_ignore_ascii_case(expected.split(';').next().unwrap_or_default().trim())
        })
}

fn key(route: &RouteContract) -> String {
    format!("{} {}", route.method.as_str(), route.operation_name)
}

#[cfg(test)]
mod tests {
    #[test]
    fn legacy_unversioned_routes_cannot_silently_discard_whole_groups() {
        let mut route = super::RouteContract::get("Group", "Group", "/group");
        route.request.query_schema = Some(
            forklaunch_core::contract::SchemaRef::named("Group")
                .with_json_schema(r#"{"type":"object"}"#),
        );
        assert!(
            super::RequestValidators::default()
                .validate_routes(&[route])
                .is_err()
        );
    }

    use super::*;
    use crate::{ForkLaunchRouter, RouterConfig};
    use forklaunch_core::ServiceInfo;
    use forklaunch_core::contract::FieldContract;
    use forklaunch_core::http::LocalRouter;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tower::ServiceExt;

    #[test]
    fn registrations_fail_closed_on_stale_or_ambiguous_schemas() {
        let route = RouteContract::get("Query", "Query", "/query")
            .with_query(FieldContract::optional("limit", "Integer"));
        let mut validators = RequestValidators::default();
        validators.insert(&route, |_, _| None).unwrap();
        assert!(validators.insert(&route, |_, _| None).is_err());
        assert!(validators.validate_routes(&[]).is_err());
        assert!(
            validators
                .validate_routes(&[route.clone(), route.clone()])
                .is_err()
        );
        let changed = route
            .clone()
            .with_query(FieldContract::optional("other", "String"));
        assert!(validators.validate_routes(&[changed]).is_err());
        assert!(validators.validate_routes(&[route]).is_ok());
    }

    #[tokio::test]
    async fn callbacks_run_once_before_local_and_axum_handlers_after_nesting() {
        for local in [false, true] {
            for mode in [
                ValidationMode::Error,
                ValidationMode::Warning,
                ValidationMode::None,
            ] {
                check_route(local, mode, false).await;
            }
            check_route(local, ValidationMode::Error, true).await;
        }
    }

    async fn check_route(local: bool, mode: ValidationMode, unauthorized: bool) {
        let validations = Arc::new(AtomicUsize::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut contract = RouteContract::get("Query", "Query", "/query")
            .with_query(FieldContract::optional("limit", "Integer"))
            .with_response(200, None);
        contract.options.request_validation = mode;
        if unauthorized {
            contract.access = forklaunch_core::contract::AccessLevel::Authenticated;
        }
        let mut validators = RequestValidators::default();
        let count = validations.clone();
        validators
            .insert(&contract, move |_, raw| {
                count.fetch_add(1, Ordering::SeqCst);
                (raw == Some("limit=bad&limit=2"))
                    .then(|| FrameworkResponse::text(400, "invalid limit"))
            })
            .unwrap();
        let count = calls.clone();
        let child = if local {
            let mut handlers = LocalRouter::new();
            handlers.register(contract.clone(), move |_: FrameworkRequest| {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(FrameworkResponse::text(200, "ok"))
            });
            ForkLaunchRouter::from_local_router(
                ServiceInfo::new("child", "1"),
                [contract],
                handlers,
            )
        } else {
            ForkLaunchRouter::new(ServiceInfo::new("child", "1")).get(contract, move || {
                let count = count.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    "ok"
                }
            })
        }
        .with_config(RouterConfig {
            request_validators: validators,
            ..RouterConfig::default()
        });
        let router = ForkLaunchRouter::new(ServiceInfo::new("parent", "1"))
            .nest("/nested", child)
            .into_router();
        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .uri("/nested/query?limit=bad&limit=2")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let rejecting = mode == ValidationMode::Error && !unauthorized;
        assert_eq!(
            response.status().as_u16(),
            if unauthorized {
                401
            } else if rejecting {
                400
            } else {
                200
            }
        );
        assert_eq!(validations.load(Ordering::SeqCst), usize::from(rejecting));
        assert_eq!(
            calls.load(Ordering::SeqCst),
            usize::from(!rejecting && !unauthorized)
        );
    }
}

#[cfg(test)]
#[path = "request_rejection_tests.rs"]
mod rejection_tests;
