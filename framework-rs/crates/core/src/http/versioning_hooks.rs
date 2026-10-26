//! Native body preprocessing/transformation around per-version schema checks.
use super::{
    BodyParserHooks, VersionFieldHooks, VersionRequestParser, VersionedRoutePlan,
    request_parser::RegisteredRequestParsers,
};
use crate::http::ContractValidationLocation;
use forklaunch_core::contract::RouteContract;
use serde_json::Value;
use std::{borrow::Cow, collections::BTreeMap, sync::Arc};

use crate::contract::schema_validation::SchemaValidationFailure;
type CheckedTransform =
    dyn Fn(Option<&Value>) -> Result<Option<Value>, SchemaValidationFailure> + Send + Sync;

type Transform = dyn Fn(Option<&Value>) -> Result<Option<Value>, String> + Send + Sync;

/// Runtime functions are deliberately separate from exported schema metadata.
#[derive(Clone, Default)]
pub struct VersionBodyHooks {
    preprocess: Option<Arc<Transform>>,
    checked_preprocess: Option<Arc<CheckedTransform>>,
    transform: Option<Arc<Transform>>,
}
impl std::fmt::Debug for VersionBodyHooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VersionBodyHooks")
            .field("preprocess", &self.preprocess.is_some())
            .field("checked_preprocess", &self.checked_preprocess.is_some())
            .field("transform", &self.transform.is_some())
            .finish()
    }
}
impl VersionBodyHooks {
    pub(super) fn is_empty(&self) -> bool {
        self.preprocess.is_none() && self.checked_preprocess.is_none() && self.transform.is_none()
    }
    /// Run before this version's schema checks. Errors reject only this version.
    #[must_use]
    pub fn with_preprocess(
        mut self,
        callback: impl Fn(&Value) -> Result<Value, String> + Send + Sync + 'static,
    ) -> Self {
        self.checked_preprocess = None;
        self.preprocess = Some(Arc::new(move |value| value.map(&callback).transpose()));
        self
    }
    /// Run after this version's schema checks. Errors reject only this version.
    #[must_use]
    pub fn with_transform(
        mut self,
        callback: impl Fn(&Value) -> Result<Value, String> + Send + Sync + 'static,
    ) -> Self {
        self.transform = Some(Arc::new(move |value| value.map(&callback).transpose()));
        self
    }
    /// Run before schema checks, including when the parsed value is absent.
    /// `None` represents absence; `Some(Value::Null)` is an explicit JSON null.
    /// Return a value to supply a missing-value default. Returning `None` still
    /// requires the selected contract to accept absence. Replaces any previously
    /// registered preprocessor, including one set with `with_preprocess`.
    #[must_use]
    pub fn with_optional_preprocess(
        mut self,
        callback: impl Fn(Option<&Value>) -> Result<Option<Value>, String> + Send + Sync + 'static,
    ) -> Self {
        self.checked_preprocess = None;
        self.preprocess = Some(Arc::new(callback));
        self
    }
    /// Run after successful schema checks, allowing the transformed output to
    /// become absent. Errors reject only this version. Replaces any previously
    /// registered transform, including one set with `with_transform`.
    #[must_use]
    pub fn with_optional_transform(
        mut self,
        callback: impl Fn(Option<&Value>) -> Result<Option<Value>, String> + Send + Sync + 'static,
    ) -> Self {
        self.transform = Some(Arc::new(callback));
        self
    }
    /// Preprocess a field while preserving evaluator failures as fatal errors.
    #[must_use]
    pub fn with_checked_field_preprocess(
        mut self,
        callback: impl Fn(Option<&Value>) -> Result<Option<Value>, SchemaValidationFailure>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.preprocess = None;
        self.checked_preprocess = Some(Arc::new(callback));
        self
    }
    pub(super) fn preprocess_field<'a>(
        &self,
        value: Option<&'a Value>,
    ) -> Result<Option<Cow<'a, Value>>, SchemaValidationFailure> {
        match &self.checked_preprocess {
            Some(callback) => callback(value).map(|value| value.map(Cow::Owned)),
            None => self
                .preprocess(value)
                .map_err(SchemaValidationFailure::Invalid),
        }
    }
    pub(super) fn preprocess<'a>(
        &self,
        body: Option<&'a Value>,
    ) -> Result<Option<Cow<'a, Value>>, String> {
        match &self.preprocess {
            Some(callback) => callback(body).map(|value| value.map(Cow::Owned)),
            None => Ok(body.map(Cow::Borrowed)),
        }
    }
    pub(super) fn transform<'a>(
        &self,
        body: Option<Cow<'a, Value>>,
    ) -> Result<Option<Cow<'a, Value>>, String> {
        match &self.transform {
            Some(callback) => callback(body.as_deref()).map(|value| value.map(Cow::Owned)),
            None => Ok(body),
        }
    }
}

#[derive(Clone, Debug)]
struct FieldHooks {
    version: String,
    location: ContractValidationLocation,
    name: String,
    hooks: VersionFieldHooks,
}

/// Hooks are indexed by HTTP method and operation name, independent of mount prefixes.
#[derive(Clone, Default, Debug)]
pub struct VersionBodyHooksRegistry {
    request_parsers: BTreeMap<String, RegisteredRequestParsers>,
    routes: BTreeMap<String, BTreeMap<String, VersionBodyHooks>>,
    parsers: BTreeMap<String, BodyParserHooks>,
    fields: BTreeMap<String, Vec<FieldHooks>>,
    simple_queries: BTreeMap<String, RouteContract>,
    json_wire: BTreeMap<String, RouteContract>,
    text_wire: BTreeMap<String, RouteContract>,
    multipart_text: BTreeMap<String, (RouteContract, crate::http::BusboyTextOptions)>,
    pre_authentication: BTreeMap<String, RouteContract>,
}
impl VersionBodyHooksRegistry {
    /// Use the same Busboy text-decoding policy as the typed multipart extractor.
    /// # Errors
    /// Rejects non-multipart/unversioned routes and duplicate registrations.
    /// Compilation rejects changed request/options/authentication metadata.
    pub fn enable_busboy_text(
        &mut self,
        route: &RouteContract,
        options: crate::http::BusboyTextOptions,
    ) -> Result<(), String> {
        VersionedRoutePlan::compile(route)?.with_busboy_text(options.clone())?;
        let key = route_key(route);
        if self.multipart_text.contains_key(&key) {
            return Err("Duplicate multipart text profile".into());
        }
        self.multipart_text.insert(key, (route.clone(), options));
        Ok(())
    }

    /// Register a trusted whole-request parser for one version. All versions must
    /// be registered before compilation; body/field hook combinations are rejected.
    /// Parser hooks and explicit wire/query/pre-auth profiles remain available.
    /// # Errors
    /// Rejects unknown/duplicate versions, conflicting hooks and stale snapshots.
    pub fn insert_request_parser(
        &mut self,
        route: &RouteContract,
        version: &str,
        parser: VersionRequestParser,
    ) -> Result<(), String> {
        if route.for_version(version).is_none()
            || self.routes.contains_key(&route_key(route))
            || self.fields.contains_key(&route_key(route))
        {
            return Err(
                "Whole-request parser requires a known version and no body/field hooks".into(),
            );
        }
        let entry = self
            .request_parsers
            .entry(route_key(route))
            .or_insert_with(|| RegisteredRequestParsers {
                route: route.clone(),
                parsers: BTreeMap::new(),
            });
        if !request_snapshot_matches(&entry.route, route) || entry.parsers.contains_key(version) {
            return Err("Stale or duplicate whole-request parser registration".into());
        }
        entry.parsers.insert(version.into(), parser);
        Ok(())
    }
    /// Use the bounded TypeScript JSON wire parser before version schema selection.
    /// The first declared version determines the exact JSON media type. Framing or
    /// media skips produce absence for body preprocessors/defaults. Unmatched
    /// None/Warning requests retain first-parser output in `VersionParsedBody`. Existing
    /// verifier/reviver hooks run once, before per-version body callbacks. Original
    /// bytes are retained; handlers consume `SelectedVersion::parsed_body`.
    /// Direct callers must supply actual HTTP framing headers; absent headers
    /// skip parsing. The MCP HTTP adapter supplies trusted framing for its own
    /// encoded bytes, without accepting transport headers from tool arguments.
    /// Finite JSON numeric/Unicode limits match `JsonRequestParsers`.
    /// Combine with `enable_selection_before_authentication` for TypeScript auth
    /// ordering. Native defaults and credential/authorization checks are unchanged.
    /// # Errors
    /// Rejects non-JSON/unversioned/invalid routes and
    /// duplicate registrations.
    /// Compilation rejects changed request/version/parser declarations.
    pub fn enable_typescript_json_wire(&mut self, route: &RouteContract) -> Result<(), String> {
        VersionedRoutePlan::compile(route)?.with_typescript_json_wire()?;
        let key = route_key(route);
        if self.json_wire.contains_key(&key) || self.text_wire.contains_key(&key) {
            return Err("Duplicate versioned JSON wire profile".into());
        }
        self.json_wire.insert(key, route.clone());
        Ok(())
    }

    /// Parse framed, exact-media text before version schema selection. Skips
    /// produce absence; framed empty input produces an empty string. Original
    /// bytes remain unchanged and `VersionParsedBody` retains decompressed bytes
    /// and the decoded string even when None/Warning modes match no version.
    /// Reuses configured charset, inflation, limits and verifier callbacks once.
    /// Combine with pre-authentication selection for TypeScript ordering.
    /// # Errors
    /// Rejects non-text/unversioned routes, JSON revivers, duplicate/conflicting
    /// profiles and stale request/version/parser metadata at compilation.
    pub fn enable_typescript_text_wire(&mut self, route: &RouteContract) -> Result<(), String> {
        VersionedRoutePlan::compile(route)?.with_typescript_text_wire()?;
        let key = route_key(route);
        if self.text_wire.contains_key(&key) || self.json_wire.contains_key(&key) {
            return Err("Duplicate or conflicting versioned text wire profile".into());
        }
        self.text_wire.insert(key, route.clone());
        Ok(())
    }

    /// Opt into version parsing/schema selection before authentication in Axum.
    /// Error-mode schema failures and fatal parser failures precede credentials;
    /// None/Warning still authenticate and authorize before invoking handlers.
    /// The local adapter uses its actual `LocalRouter` plan and caches selection.
    /// Native defaults remain post-authentication. Raw credentials are unchanged.
    /// # Errors
    /// Rejects unversioned/invalid/duplicate registrations. Changed schemas are
    /// rejected at router construction; mounted path prefixes are allowed.
    pub fn enable_selection_before_authentication(
        &mut self,
        route: &RouteContract,
    ) -> Result<(), String> {
        if route.versions.is_empty() {
            return Err("Pre-auth selection requires a versioned route".into());
        }
        VersionedRoutePlan::compile(route)?;
        let key = route_key(route);
        if self.pre_authentication.contains_key(&key) {
            return Err("Duplicate pre-auth version selection".into());
        }
        self.pre_authentication.insert(key, route.clone());
        Ok(())
    }
    /// Whether this registered operation opts into pre-authentication selection.
    #[must_use]
    pub fn selection_before_authentication(&self, route: &RouteContract) -> bool {
        self.pre_authentication.contains_key(&route_key(route))
    }
    /// Parse raw HTTP query strings with the default Express/Node simple parser
    /// before version field schemas and hooks. Parsing runs once per selection,
    /// preserving duplicate arrays and the 1000-segment admission limit.
    /// Native query maps and original raw strings remain unchanged. Local callers
    /// must supply `FrameworkRequest::raw_query`; absence means an empty query.
    /// This is not a projection of MCP JSON arguments or extended/custom query
    /// parsers. MCP adapters do not automatically populate this HTTP-only input.
    /// # Errors
    /// Rejects unversioned/invalid contracts and duplicate registrations. Compile
    /// rejects changed request/version schemas; mount prefixes may still change.
    pub fn enable_typescript_simple_query(&mut self, route: &RouteContract) -> Result<(), String> {
        if route.versions.is_empty() {
            return Err("Simple query version projection requires a versioned route".into());
        }
        VersionedRoutePlan::compile(route)?;
        let key = route_key(route);
        if self.simple_queries.contains_key(&key) {
            return Err("Duplicate simple query version projection".into());
        }
        self.simple_queries.insert(key, route.clone());
        Ok(())
    }
    /// # Errors
    /// Rejects unknown versions and versions with no declared body.
    pub fn insert(
        &mut self,
        route: &RouteContract,
        version: &str,
        hooks: VersionBodyHooks,
    ) -> Result<(), String> {
        if self.request_parsers.contains_key(&route_key(route)) {
            return Err("Whole-request parsers cannot combine with body/field hooks".into());
        }
        let projected = route.for_version(version).ok_or("Unknown hook version")?;
        if projected.request.body.is_none() {
            return Err("Body hooks require a declared body".into());
        }
        self.routes
            .entry(route_key(route))
            .or_default()
            .insert(version.into(), hooks);
        Ok(())
    }
    /// Register callbacks for one declared parameter, query or header field.
    /// # Errors
    /// Rejects invalid contracts, unknown versions or undeclared fields.
    pub fn insert_field(
        &mut self,
        route: &RouteContract,
        version: &str,
        location: ContractValidationLocation,
        name: &str,
        hooks: VersionFieldHooks,
    ) -> Result<(), String> {
        if self.request_parsers.contains_key(&route_key(route)) {
            return Err("Whole-request parsers cannot combine with body/field hooks".into());
        }
        VersionedRoutePlan::compile(route)?.with_field_hooks(
            version,
            location,
            name,
            hooks.clone(),
        )?;
        let name = if location == ContractValidationLocation::Header {
            name.to_ascii_lowercase()
        } else {
            name.to_owned()
        };
        let fields = self.fields.entry(route_key(route)).or_default();
        fields.retain(|field| {
            field.version != version || field.location != location || field.name != name
        });
        fields.push(FieldHooks {
            version: version.into(),
            location,
            name,
            hooks,
        });
        Ok(())
    }
    /// # Errors
    /// Rejects routes without a supported shared body parser.
    pub fn insert_parser(
        &mut self,
        route: &RouteContract,
        hooks: BodyParserHooks,
    ) -> Result<(), String> {
        VersionedRoutePlan::compile(route)?.with_parser_hooks(hooks.clone())?;
        self.parsers.insert(route_key(route), hooks);
        Ok(())
    }
    /// Check that each registered operation identifies exactly one route.
    /// # Errors
    /// Rejects unused hooks and ambiguous method/operation pairs.
    pub fn validate_routes(&self, routes: &[RouteContract]) -> Result<(), String> {
        let mut counts = BTreeMap::<String, usize>::new();
        for route in routes {
            *counts.entry(route_key(route)).or_default() += 1;
        }
        for key in self
            .routes
            .keys()
            .chain(self.request_parsers.keys())
            .chain(self.parsers.keys())
            .chain(self.fields.keys())
            .chain(self.simple_queries.keys())
            .chain(self.json_wire.keys())
            .chain(self.text_wire.keys())
            .chain(self.multipart_text.keys())
            .chain(self.pre_authentication.keys())
        {
            if counts.get(key) != Some(&1) {
                return Err(format!(
                    "Version hook operation must identify exactly one route: {key}"
                ));
            }
        }
        Ok(())
    }
    /// Combine hook registries when mounting routers.
    /// # Panics
    /// Panics if both registries define hooks for the same method and operation.
    pub fn extend(&mut self, other: Self) {
        for (key, value) in other.request_parsers {
            assert!(
                self.request_parsers.insert(key, value).is_none(),
                "duplicate whole-request parser operation"
            );
        }
        for (key, entry) in other.multipart_text {
            assert!(
                self.multipart_text.insert(key, entry).is_none(),
                "duplicate multipart text profile"
            );
        }
        for (key, route) in other.text_wire {
            assert!(
                !self.json_wire.contains_key(&key),
                "conflicting wire profile"
            );
            assert!(
                self.text_wire.insert(key, route).is_none(),
                "duplicate text wire operation"
            );
        }
        for (key, route) in other.json_wire {
            assert!(
                !self.text_wire.contains_key(&key),
                "conflicting wire profile"
            );
            assert!(
                self.json_wire.insert(key, route).is_none(),
                "duplicate JSON wire operation"
            );
        }
        for (key, route) in other.pre_authentication {
            assert!(
                self.pre_authentication.insert(key, route).is_none(),
                "duplicate pre-auth selection operation"
            );
        }
        for (key, route) in other.simple_queries {
            assert!(
                self.simple_queries.insert(key, route).is_none(),
                "duplicate simple query operation"
            );
        }
        for (key, hooks) in other.fields {
            assert!(
                self.fields.insert(key, hooks).is_none(),
                "duplicate field hook operation"
            );
        }
        for (key, hooks) in other.parsers {
            assert!(
                self.parsers.insert(key, hooks).is_none(),
                "duplicate parser hook operation"
            );
        }
        for (key, hooks) in other.routes {
            assert!(
                self.routes.insert(key, hooks).is_none(),
                "duplicate body hook operation"
            );
        }
    }
    /// # Errors
    /// Rejects invalid schemas or hooks referring to removed versions.
    pub fn compile(&self, route: &RouteContract) -> Result<VersionedRoutePlan, String> {
        let mut plan = VersionedRoutePlan::compile(route)?;
        if self.request_parsers.contains_key(&route_key(route))
            && (self.routes.contains_key(&route_key(route))
                || self.fields.contains_key(&route_key(route)))
        {
            return Err("Whole-request parsers conflict with body/field hook registrations".into());
        }
        if let Some(snapshot) = self.pre_authentication.get(&route_key(route))
            && (snapshot.versions != route.versions
                || snapshot.request != route.request
                || snapshot.auth != route.auth
                || snapshot.rbac != route.rbac
                || snapshot.access != route.access)
        {
            return Err("Stale pre-auth version selection registration".into());
        }
        if let Some(snapshot) = self.simple_queries.get(&route_key(route)) {
            if snapshot.versions != route.versions || snapshot.request != route.request {
                return Err("Stale simple query version projection".into());
            }
            plan.typescript_simple_query = true;
        }
        if let Some(versions) = self.routes.get(&route_key(route)) {
            for (version, hooks) in versions {
                plan = plan.with_body_hooks(version, hooks.clone())?;
            }
        }
        if let Some(fields) = self.fields.get(&route_key(route)) {
            for field in fields {
                plan = plan.with_field_hooks(
                    &field.version,
                    field.location,
                    &field.name,
                    field.hooks.clone(),
                )?;
            }
        }
        if let Some(hooks) = self.parsers.get(&route_key(route)) {
            plan = plan.with_parser_hooks(hooks.clone())?;
        }
        if let Some(snapshot) = self.json_wire.get(&route_key(route)) {
            if snapshot.versions != route.versions
                || snapshot.request != route.request
                || snapshot.options.body_parser != route.options.body_parser
            {
                return Err("Stale versioned JSON wire profile".into());
            }
            plan = plan.with_typescript_json_wire()?;
        }
        if let Some(snapshot) = self.text_wire.get(&route_key(route)) {
            if snapshot.versions != route.versions
                || snapshot.request != route.request
                || snapshot.options.body_parser != route.options.body_parser
            {
                return Err("Stale versioned text wire profile".into());
            }
            plan = plan.with_typescript_text_wire()?;
        }
        if let Some((snapshot, options)) = self.multipart_text.get(&route_key(route)) {
            if !request_snapshot_matches(snapshot, route) {
                return Err("Stale multipart text profile".into());
            }
            plan = plan.with_busboy_text(options.clone())?;
        }
        if let Some(entry) = self.request_parsers.get(&route_key(route)) {
            if !request_snapshot_matches(&entry.route, route) {
                return Err("Stale whole-request parser registration".into());
            }
            plan = plan.with_request_parsers(entry.parsers.clone())?;
        }
        Ok(plan)
    }
}
fn request_snapshot_matches(left: &RouteContract, right: &RouteContract) -> bool {
    left.request == right.request
        && left.versions == right.versions
        && left.options == right.options
        && left.auth == right.auth
        && left.rbac == right.rbac
        && left.access == right.access
}
fn route_key(route: &RouteContract) -> String {
    format!("{} {}", route.method.as_str(), route.operation_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{Body, FrameworkRequest, HttpMethod, RequestContext};
    use forklaunch_core::contract::{BodyContract, RequestContract, VersionedContract};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn hooks_reject_invalid_registration_and_transform_only_valid_inputs() {
        let mut route = RouteContract::post("Hook", "Hook", "/hook");
        route.options.body_parser.json_strict = false;
        route = route.with_version(VersionedContract {
            version: "one".into(),
            request: RequestContract::default().with_body(BodyContract {
                schema: forklaunch_core::contract::SchemaRef::named("Input")
                    .with_json_schema(r#"{"type":"integer"}"#),
                ..BodyContract::json("Input")
            }),
            responses: vec![],
        });
        let mut registry = VersionBodyHooksRegistry::default();
        assert!(
            registry
                .insert(&route, "missing", VersionBodyHooks::default())
                .is_err()
        );
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        registry
            .insert(
                &route,
                "one",
                VersionBodyHooks::default().with_transform(move |value| {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(value.clone())
                }),
            )
            .unwrap();
        assert!(registry.validate_routes(&[]).is_err());
        assert!(
            registry
                .validate_routes(&[route.clone(), route.clone()])
                .is_err()
        );
        assert!(
            registry
                .validate_routes(std::slice::from_ref(&route))
                .is_ok()
        );
        let plan = registry.compile(&route).unwrap();
        let request = FrameworkRequest::new(HttpMethod::Post, "/hook", RequestContext::new("hook"))
            .with_header("content-type", "application/json")
            .with_body(Body::Text("false".into()));
        assert!(plan.select(&request).is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        route.versions.get_mut("one").unwrap().request.body = None;
        assert!(registry.compile(&route).is_err());
        assert!(
            registry
                .insert(&route, "one", VersionBodyHooks::default())
                .is_err()
        );
    }
}
