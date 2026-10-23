//! Schema-selected versioned routes with one authorization boundary.
use super::{
    AccessLevel, Arc, BoundHandler, ContractRouter, Endpoint, EndpointError, EndpointPolicy,
    Future, MethodFilter, NativeBody, Public, Request, RouteContract, State, compile_handler,
    fields, on, to_bytes,
};
use forklaunch_core::contract::endpoint::VersionedEndpoint;
use forklaunch_core::http::{
    ContractValidationLocation, FrameworkRequest, RequestContext, VersionedRoutePlan,
};
use std::collections::{BTreeMap, HashMap};

/// Each published version must have a handler whose Rust values match its descriptor.
pub struct VersionedHandlers<S, C = ()> {
    pub(super) route: RouteContract,
    #[cfg(feature = "native-streams")]
    pub(super) stream_options: Option<super::streams::StreamOptions>,
    pub(super) handlers: BTreeMap<String, BoundHandler<S, C>>,
}
impl<S: Clone + Send + Sync + 'static, C: Send + 'static> VersionedHandlers<S, C> {
    #[must_use]
    pub fn new(endpoint: VersionedEndpoint) -> Self {
        Self {
            route: endpoint.into_contract(),
            #[cfg(feature = "native-streams")]
            stream_options: None,
            handlers: BTreeMap::new(),
        }
    }
    /// # Errors
    /// Rejects unknown/duplicate versions and descriptor drift from the publication.
    pub fn bind<I, O, F, Fut>(
        mut self,
        version: &str,
        endpoint: Endpoint<I, O>,
        handler: F,
    ) -> Result<Self, String>
    where
        I: NativeBody,
        O: NativeBody,
        F: Fn(State<S>, C, I::Value) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<O::Value, EndpointError>> + Send + 'static,
    {
        let outputs = endpoint.output_contracts();
        let route = endpoint.into_contract();
        if self.handlers.contains_key(version)
            || self.route.for_version(version).as_ref() != Some(&route)
        {
            return Err("version binding differs from published contract or is duplicated".into());
        }
        self.handlers.insert(
            version.into(),
            compile_handler::<S, I, O, C, F, Fut>(&route, &outputs, handler)?,
        );
        Ok(self)
    }
}
impl<S: Clone + Send + Sync + 'static> ContractRouter<S> {
    /// Bind all public versions; schema matching selects the last declaration.
    /// # Errors
    /// Rejects private, missing, duplicate or inconsistent version declarations.
    pub fn bind_versioned(self, handlers: VersionedHandlers<S>) -> Result<Self, String> {
        if handlers.route.access != AccessLevel::Public
            || handlers.route.auth.is_some()
            || handlers.route.rbac.is_some()
        {
            return Err("private endpoint requires an authorization policy".into());
        }
        self.bind_versioned_authorized(handlers, Public)
    }
    /// Authorization and exact-wire body verification run once, before selection.
    /// # Errors
    /// Rejects missing handlers, divergent declarations and incompatible body parsers.
    #[allow(clippy::too_many_lines)] // Keep admission and guarded dispatch together.
    pub fn bind_versioned_authorized<P: EndpointPolicy<S>>(
        mut self,
        handlers: VersionedHandlers<S, P::Context>,
        policy: P,
    ) -> Result<Self, String> {
        #[cfg(feature = "native-streams")]
        let stream_options = handlers.stream_options;
        let route = handlers.route;
        P::CAPABILITIES.admit(&route)?;
        if route.versions.is_empty() || handlers.handlers.len() != route.versions.len() {
            return Err("every published version requires a typed handler".into());
        }
        let candidates: Vec<_> = self
            .registry
            .routes()
            .iter()
            .enumerate()
            .filter(|(_, candidate)| {
                candidate.method == route.method && candidate.path == route.path
            })
            .collect();
        if candidates.len() != 1
            || candidates[0].1 != &route
            || self.mounted.contains(&candidates[0].0)
        {
            return Err(
                "versioned endpoint differs from published contract or is already bound".into(),
            );
        }
        let index = candidates[0].0;
        let mut plan = VersionedRoutePlan::compile(&route)?;
        for (name, version) in &route.versions {
            for (location, fields) in [
                (
                    ContractValidationLocation::PathParameter,
                    &route.request.params,
                ),
                (ContractValidationLocation::Query, &version.request.query),
                (ContractValidationLocation::Header, &version.request.headers),
            ] {
                for field in fields {
                    plan = plan.with_field_hooks(
                        name,
                        location,
                        &field.name,
                        fields::version_hook(
                            field,
                            location == ContractValidationLocation::Header
                                && matches!(
                                    field.name.to_ascii_lowercase().as_str(),
                                    "content-type" | "content-encoding"
                                ),
                        )?,
                    )?;
                }
            }
        }
        let response_plans = Arc::new(
            route
                .versions
                .keys()
                .map(|name| {
                    let contract = route.for_version(name).ok_or("missing version contract")?;
                    Ok((
                        name.clone(),
                        Arc::new(super::responses::configure(
                            super::responses::version_plans(&contract.responses)?,
                            &contract,
                        )),
                    ))
                })
                .collect::<Result<BTreeMap<_, _>, String>>()?,
        );
        let method: axum::http::Method = route
            .method
            .as_str()
            .parse()
            .map_err(|_| "invalid method")?;
        let filter = MethodFilter::try_from(method).map_err(|_| "unsupported native method")?;
        let path = route.openapi_path.clone();
        let route = Arc::new(route);
        let plan = Arc::new(plan);
        let handlers = Arc::new(handlers.handlers);
        self.router = self.router.route(
            &path,
            on(filter, move |State(state): State<S>, request: Request| {
                let policy = policy.clone();
                let route = route.clone();
                let plan = plan.clone();
                let handlers = handlers.clone();
                let response_plans = response_plans.clone();
                #[cfg(feature = "native-streams")]
                let stream_options = stream_options.clone();
                async move {
                    let (mut parts, body) = request.into_parts();
                    let context = policy.authorize(&mut parts, &state, &route).await?;
                    let read = to_bytes(body, 2 * 1024 * 1024);
                    #[cfg(feature = "native-streams")]
                    let bytes = if let Some(options) = stream_options {
                        options
                            .lifecycle
                            .operation(read)
                            .await
                            .map_err(|_| EndpointError::PayloadTooLarge)?
                    } else {
                        read.await.map_err(|_| EndpointError::PayloadTooLarge)?
                    };
                    #[cfg(not(feature = "native-streams"))]
                    let bytes = read.await.map_err(|_| EndpointError::PayloadTooLarge)?;
                    let context = policy
                        .authorize_body(context, &mut parts, &bytes, &state, &route)
                        .await?;
                    let mut request = FrameworkRequest::new(
                        route.method,
                        parts.uri.path(),
                        RequestContext::new("native-version-selection"),
                    );
                    request.body = if bytes.is_empty() {
                        forklaunch_core::http::Body::Empty
                    } else {
                        forklaunch_core::http::Body::Binary(bytes.to_vec())
                    };
                    request.raw_query = parts.uri.query().map(str::to_owned);
                    let pairs: Vec<(String, String)> =
                        serde_urlencoded::from_str(parts.uri.query().unwrap_or(""))
                            .map_err(|_| EndpointError::InvalidRequest)?;
                    let mut query = BTreeMap::<String, Vec<String>>::new();
                    for (name, value) in pairs {
                        query.entry(name).or_default().push(value);
                    }
                    for (name, values) in query {
                        request.query.insert(name, encode_values(&values)?);
                    }
                    for name in parts.headers.keys() {
                        let values = parts
                            .headers
                            .get_all(name)
                            .iter()
                            .map(|value| value.to_str().map(str::to_owned))
                            .collect::<Result<Vec<_>, _>>()
                            .map_err(|_| EndpointError::InvalidRequest)?;
                        // The parser consumes the real content type; field hooks consume a list.
                        request
                            .headers
                            .insert(name.to_string(), encode_values(&values)?);
                    }
                    // Parser headers retain their original representation. Duplicate parser
                    // headers are ambiguous and rejected before schema selection.
                    for name in ["content-type", "content-encoding"] {
                        if parts.headers.get_all(name).iter().count() > 1 {
                            return Err(EndpointError::InvalidRequest);
                        }
                        if let Some(value) = parts.headers.get(name) {
                            request.headers.insert(
                                name.into(),
                                value
                                    .to_str()
                                    .map_err(|_| EndpointError::InvalidRequest)?
                                    .into(),
                            );
                        }
                    }
                    if !route.request.params.is_empty() {
                        use axum::extract::FromRequestParts;
                        let axum::extract::Path(values) =
                            axum::extract::Path::<HashMap<String, String>>::from_request_parts(
                                &mut parts, &state,
                            )
                            .await
                            .map_err(|_| EndpointError::InvalidRequest)?;
                        for (name, value) in values {
                            request.params.insert(name, encode_values(&[value])?);
                        }
                    }
                    let (_, selected) = plan
                        .select(&request)
                        .map_err(|_| EndpointError::InvalidRequest)?
                        .ok_or(EndpointError::InvalidRequest)?;
                    let matched = selected
                        .matched_versions
                        .iter()
                        .map(|name| {
                            response_plans
                                .get(name)
                                .cloned()
                                .ok_or(EndpointError::InvalidResponse)
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    parts
                        .extensions
                        .insert(super::responses::MatchedResponsePlans(matched));
                    let execute = handlers
                        .get(&selected.version)
                        .ok_or(EndpointError::InvalidRequest)?;
                    execute(state, context, parts, bytes).await
                }
            }),
        );
        self.mounted.push(index);
        Ok(self)
    }
}
fn encode_values(values: &[String]) -> Result<String, EndpointError> {
    serde_json::to_string(values).map_err(|_| EndpointError::InvalidRequest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::endpoint::{Body, JsonBody, NoBody, Parts, TextBody};
    use forklaunch_core::contract::{
        ContractRegistry, HttpMethod,
        endpoint::{Fields, NoFields},
    };
    use tower::ServiceExt;
    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    struct First {
        #[schemars(length(min = 2))]
        title: String,
    }
    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    struct Second {
        #[schemars(length(min = 5))]
        title: String,
    }
    fn first() -> Endpoint<JsonBody<First>, TextBody> {
        Endpoint::new("Versioned", "Versioned", HttpMethod::Post, "/versions", 200).unwrap()
    }
    fn second() -> Endpoint<JsonBody<Second>, TextBody> {
        Endpoint::new("Versioned", "Versioned", HttpMethod::Post, "/versions", 200).unwrap()
    }
    fn registry(endpoint: &VersionedEndpoint) -> ContractRegistry {
        let mut registry = ContractRegistry::new();
        registry.register(endpoint.contract());
        registry
    }
    #[tokio::test]
    async fn last_schema_match_and_numeric_declaration_order_select_typed_handler() {
        for (first_name, second_name, overlap) in [("old", "new", "second"), ("10", "2", "first")] {
            let a = first();
            let b = second();
            let descriptor = VersionedEndpoint::new(first_name, &a)
                .unwrap()
                .with_version(second_name, &b)
                .unwrap();
            let handlers = VersionedHandlers::new(descriptor.clone())
                .bind(first_name, a, |_, (), value| async move {
                    Ok(format!("first:{}", value.title))
                })
                .unwrap()
                .bind(second_name, b, |_, (), value| async move {
                    Ok(format!("second:{}", value.title))
                })
                .unwrap();
            let app = ContractRouter::<()>::new(registry(&descriptor))
                .bind_versioned(handlers)
                .unwrap()
                .finish()
                .unwrap();
            for (title, status, prefix) in [
                ("x", 400, ""),
                ("abc", 200, "first"),
                ("longer", 200, overlap),
            ] {
                let response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri("/versions")
                            .header("content-type", "application/json")
                            .body(Body::from(format!("{{\"title\":\"{title}\"}}")))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status().as_u16(), status);
                if status == 200 {
                    assert_eq!(
                        to_bytes(response.into_body(), 1024).await.unwrap(),
                        format!("{prefix}:{title}")
                    );
                }
            }
        }
    }
    #[derive(serde::Deserialize, schemars::JsonSchema)]
    struct Query {
        count: u32,
        tags: Vec<String>,
    }
    #[derive(serde::Deserialize, schemars::JsonSchema)]
    struct Headers {
        #[serde(rename = "x-number")]
        number: u32,
        #[serde(rename = "content-type")]
        content_type: String,
    }
    #[derive(serde::Deserialize, schemars::JsonSchema)]
    struct Path {
        id: u32,
    }
    #[tokio::test]
    async fn typed_metadata_coercion_matches_selection_and_execution() {
        type Input = Fields<NoBody, Query, Headers, Path>;
        let typed =
            Endpoint::<Input, TextBody>::new("Read", "Read", HttpMethod::Get, "/items/:id", 200)
                .unwrap();
        let descriptor = VersionedEndpoint::new("v1", &typed).unwrap();
        let handlers = VersionedHandlers::new(descriptor.clone())
            .bind("v1", typed, |_, (), value| async move {
                Ok(format!(
                    "{}:{}:{}:{}:{}",
                    value.path.id,
                    value.query.count,
                    value.headers.number,
                    value.query.tags.join(","),
                    value.headers.content_type
                ))
            })
            .unwrap();
        let app = ContractRouter::<()>::new(registry(&descriptor))
            .bind_versioned(handlers)
            .unwrap()
            .finish()
            .unwrap();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/items/42?count=7&tags=a&tags=b")
                    .header("x-number", "9")
                    .header("content-type", "text/plain")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            to_bytes(response.into_body(), 1024).await.unwrap(),
            "42:7:9:a,b:text/plain"
        );
        let invalid = app
            .oneshot(
                Request::builder()
                    .uri("/items/42?count=7&count=8&tags=a")
                    .header("x-number", "9")
                    .header("content-type", "text/plain")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(invalid.status(), 400);
    }
    #[test]
    fn rejects_incomplete_duplicate_and_divergent_bindings() {
        let a = first();
        let b = second();
        let descriptor = VersionedEndpoint::new("v1", &a)
            .unwrap()
            .with_version("v2", &b)
            .unwrap();
        assert!(descriptor.clone().with_version("v1", &a).is_err());
        assert!(
            descriptor
                .clone()
                .with_version("v3", &a.clone().with_access(AccessLevel::Internal))
                .is_err()
        );
        let handlers = VersionedHandlers::<()>::new(descriptor.clone())
            .bind("v1", a.clone(), |_, (), _| async { Ok("one".into()) })
            .unwrap();
        assert!(
            ContractRouter::new(registry(&descriptor))
                .bind_versioned(handlers)
                .is_err()
        );
        let handlers = VersionedHandlers::<()>::new(descriptor.clone())
            .bind("v1", a.clone(), |_, (), _| async { Ok("one".into()) })
            .unwrap();
        assert!(
            handlers
                .bind("v1", a, |_, (), _| async { Ok("two".into()) })
                .is_err()
        );
        assert!(
            VersionedHandlers::<()>::new(descriptor)
                .bind("v1", b, |_, (), _| async { Ok("wrong".into()) })
                .is_err()
        );
        let _ = std::mem::size_of::<NoFields>();
    }
    #[derive(Clone)]
    struct Verify(std::sync::Arc<std::sync::atomic::AtomicUsize>);
    #[allow(clippy::unused_async_trait_impl)]
    impl EndpointPolicy<()> for Verify {
        const CAPABILITIES: super::super::PolicyCapabilities =
            super::super::PolicyCapabilities::SignedInternal;
        type Context = String;
        async fn authorize(
            &self,
            _: &mut Parts,
            (): &(),
            _: &RouteContract,
        ) -> Result<String, EndpointError> {
            Ok("verified".into())
        }
        async fn authorize_body(
            &self,
            context: String,
            _: &mut Parts,
            bytes: &[u8],
            (): &(),
            _: &RouteContract,
        ) -> Result<String, EndpointError> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if bytes == br#"{"title":"signed"}"# {
                Ok(context)
            } else {
                Err(EndpointError::Unauthorized)
            }
        }
    }
    #[tokio::test]
    async fn body_authorization_runs_once_before_any_version_schema() {
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let a = first().with_access(AccessLevel::Internal);
        let b = second().with_access(AccessLevel::Internal);
        let descriptor = VersionedEndpoint::new("v1", &a)
            .unwrap()
            .with_version("v2", &b)
            .unwrap();
        let handlers = VersionedHandlers::new(descriptor.clone())
            .bind("v1", a, |_, context: String, _| async move { Ok(context) })
            .unwrap()
            .bind("v2", b, |_, context: String, _| async move { Ok(context) })
            .unwrap();
        let app = ContractRouter::<()>::new(registry(&descriptor))
            .bind_versioned_authorized(handlers, Verify(count.clone()))
            .unwrap()
            .finish()
            .unwrap();
        for (body, status) in [(r#"{"title":"signed"}"#, 200), ("malformed-secret", 401)] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/versions")
                        .header("content-type", "application/json")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), status);
        }
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }
    #[derive(serde::Deserialize, schemars::JsonSchema)]
    struct OldQuery {
        #[schemars(regex(pattern = "^(both|first)$"))]
        select: String,
        body: String,
        header: String,
    }
    #[derive(serde::Deserialize, schemars::JsonSchema)]
    struct NewQuery {
        #[schemars(regex(pattern = "^(both|last)$"))]
        select: String,
        body: String,
        header: String,
    }
    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    struct OldOutput {
        #[schemars(regex(pattern = "^old$"))]
        value: String,
    }
    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    struct NewOutput {
        #[schemars(regex(pattern = "^new$"))]
        value: String,
    }
    #[derive(serde::Serialize, schemars::JsonSchema)]
    struct OldHeader {
        #[serde(rename = "x-branch")]
        #[schemars(regex(pattern = "^old$"))]
        value: String,
    }
    #[derive(serde::Serialize, schemars::JsonSchema)]
    struct NewHeader {
        #[serde(rename = "x-branch")]
        #[schemars(regex(pattern = "^new$"))]
        value: String,
    }
    #[tokio::test]
    async fn matched_version_bodies_and_headers_union_independently() {
        use forklaunch_core::contract::endpoint::ResponseWithHeaders;
        type OldInput = Fields<NoBody, OldQuery>;
        type NewInput = Fields<NoBody, NewQuery>;
        let old = Endpoint::<OldInput, JsonBody<OldOutput>>::new(
            "Union",
            "Union",
            HttpMethod::Get,
            "/union",
            200,
        )
        .unwrap()
        .with_typed_response_headers::<OldHeader>()
        .unwrap();
        let new = Endpoint::<NewInput, JsonBody<NewOutput>>::new(
            "Union",
            "Union",
            HttpMethod::Get,
            "/union",
            200,
        )
        .unwrap()
        .with_typed_response_headers::<NewHeader>()
        .unwrap();
        let descriptor = VersionedEndpoint::new("first", &old)
            .unwrap()
            .with_version("last", &new)
            .unwrap();
        let handlers = VersionedHandlers::new(descriptor.clone())
            .bind("first", old, |_, (), value| {
                assert!(matches!(value.query.select.as_str(), "both" | "first"));
                std::future::ready(Ok(ResponseWithHeaders {
                    body: OldOutput {
                        value: value.query.body,
                    },
                    headers: OldHeader {
                        value: value.query.header,
                    },
                }))
            })
            .unwrap()
            .bind("last", new, |_, (), value| {
                assert!(matches!(value.query.select.as_str(), "both" | "last"));
                std::future::ready(Ok(ResponseWithHeaders {
                    body: NewOutput {
                        value: value.query.body,
                    },
                    headers: NewHeader {
                        value: value.query.header,
                    },
                }))
            })
            .unwrap();
        let app = ContractRouter::<()>::new(registry(&descriptor))
            .bind_versioned(handlers)
            .unwrap()
            .finish()
            .unwrap();
        for (selected, body, header, status) in [
            ("both", "old", "new", 200),
            ("both", "new", "old", 200),
            ("first", "new", "old", 500),
            ("last", "old", "new", 500),
            ("both", "invalid", "new", 500),
            ("both", "old", "invalid", 500),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!(
                            "/union?select={selected}&body={body}&header={header}"
                        ))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                response.status().as_u16(),
                status,
                "{selected}/{body}/{header}"
            );
            if status == 200 {
                assert_eq!(response.headers()["x-branch"], header);
            }
        }
    }
}
