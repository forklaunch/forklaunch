//! Versioned SSE shares finite version dispatch and the stream authorization lifecycle.
use super::super::{BoundHandler, VersionedHandlers, responses::MatchedResponsePlans};
use super::{
    Arc, AuthorizationFuture, BTreeMap, ContractRouter, Endpoint, EndpointError, EndpointPolicy,
    Future, HttpMethod, IntoResponse, KeepAlive, NativeBody, RequestPlan, RouteContract, Serialize,
    ServerEvent, Sse, SseBody, State, Stream, StreamContext, StreamExt, StreamOptions,
};
use forklaunch_core::contract::endpoint::{
    ResponseWithHeaders, VersionedEndpoint, WithResponseHeaders,
};

/// All versions have typed producers; selection and authorization remain framework-owned.
pub struct VersionedSseHandlers<S, C> {
    inner: VersionedHandlers<S, StreamContext<C>>,
    options: StreamOptions,
}
impl<S: Clone + Send + Sync + 'static, C: Send + 'static> VersionedSseHandlers<S, C> {
    #[must_use]
    pub fn new(endpoint: VersionedEndpoint, options: StreamOptions) -> Self {
        let mut inner = VersionedHandlers::new(endpoint);
        inner.stream_options = Some(options.clone());
        Self { inner, options }
    }
    /// # Errors
    /// Rejects duplicate or divergent bindings and unsupported stream representations.
    pub fn bind<I, T, F, Fut, Events>(
        mut self,
        version: &str,
        endpoint: Endpoint<I, SseBody<T>>,
        handler: F,
    ) -> Result<Self, String>
    where
        I: NativeBody,
        T: forklaunch_core::contract::__private::schemars::JsonSchema + Serialize + Send + 'static,
        F: Fn(State<S>, StreamContext<C>, I::Value) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<Events, EndpointError>> + Send + 'static,
        Events: Stream<Item = Result<ServerEvent<T>, EndpointError>> + Send + 'static,
    {
        let outputs = endpoint.output_contracts();
        let route = endpoint.into_contract();
        if self.inner.handlers.contains_key(version)
            || self.inner.route.for_version(version).as_ref() != Some(&route)
        {
            return Err(
                "versioned stream binding differs from published contract or is duplicated".into(),
            );
        }
        let bound = compile::<S, C, I, T, _, _, Events>(
            &route,
            &outputs,
            self.options.clone(),
            move |state, context, input| {
                let future = handler(state, context, input);
                async move { Ok((future.await?, BTreeMap::new())) }
            },
        )?;
        self.inner.handlers.insert(version.into(), bound);
        Ok(self)
    }
    /// Bind a producer whose dynamic headers share the matched-version header union.
    /// # Errors
    /// Rejects duplicate/divergent bindings or unsupported response representations.
    pub fn bind_with_headers<I, T, H, F, Fut, Events>(
        mut self,
        version: &str,
        endpoint: Endpoint<I, WithResponseHeaders<SseBody<T>, H>>,
        handler: F,
    ) -> Result<Self, String>
    where
        I: NativeBody,
        T: forklaunch_core::contract::__private::schemars::JsonSchema + Serialize + Send + 'static,
        H: forklaunch_core::contract::__private::schemars::JsonSchema + Serialize + Send + 'static,
        F: Fn(State<S>, StreamContext<C>, I::Value) -> Fut + Clone + Send + Sync + 'static,
        Fut:
            Future<Output = Result<ResponseWithHeaders<Events, H>, EndpointError>> + Send + 'static,
        Events: Stream<Item = Result<ServerEvent<T>, EndpointError>> + Send + 'static,
    {
        let outputs = endpoint.output_contracts();
        let route = endpoint.into_contract();
        if self.inner.handlers.contains_key(version)
            || self.inner.route.for_version(version).as_ref() != Some(&route)
        {
            return Err(
                "versioned stream binding differs from published contract or is duplicated".into(),
            );
        }
        let bound = compile::<S, C, I, T, _, _, Events>(
            &route,
            &outputs,
            self.options.clone(),
            move |state, context, input| {
                let future = handler(state, context, input);
                async move {
                    let response = future.await?;
                    let headers = forklaunch_core::contract::endpoint::encode_response_headers(
                        response.headers,
                    )
                    .map_err(|_| EndpointError::InvalidResponse)?;
                    Ok((response.body, headers))
                }
            },
        )?;
        self.inner.handlers.insert(version.into(), bound);
        Ok(self)
    }
}
impl<S: Clone + Send + Sync + 'static> ContractRouter<S> {
    /// Bind typed schema-selected SSE producers with initial and periodic authorization.
    /// # Errors
    /// Rejects missing versions, contract drift, and incompatible request parsers.
    pub fn bind_versioned_sse_authorized<P: EndpointPolicy<S>>(
        self,
        handlers: VersionedSseHandlers<S, P::Context>,
        policy: P,
    ) -> Result<Self, String> {
        let policy = StreamingPolicy {
            policy,
            options: handlers.options,
        };
        self.bind_versioned_authorized(handlers.inner, policy)
    }
}
#[derive(Clone)]
struct StreamingPolicy<P> {
    policy: P,
    options: StreamOptions,
}
impl<S: Clone + Send + Sync + 'static, P: EndpointPolicy<S>> EndpointPolicy<S>
    for StreamingPolicy<P>
{
    const CAPABILITIES: crate::native::PolicyCapabilities = P::CAPABILITIES;
    type Context = StreamContext<P::Context>;
    async fn authorize(
        &self,
        parts: &mut axum::http::request::Parts,
        state: &S,
        route: &RouteContract,
    ) -> Result<Self::Context, EndpointError> {
        let initial = self
            .options
            .lifecycle
            .operation(async {
                Ok::<_, std::convert::Infallible>(self.policy.authorize(parts, state, route).await)
            })
            .await
            .map_err(|_| EndpointError::Unavailable)??;
        Ok(context(
            initial,
            parts,
            state,
            route,
            &self.policy,
            &self.options,
        ))
    }
    async fn authorize_body(
        &self,
        initial: Self::Context,
        parts: &mut axum::http::request::Parts,
        bytes: &[u8],
        state: &S,
        route: &RouteContract,
    ) -> Result<Self::Context, EndpointError> {
        let initial = self
            .options
            .lifecycle
            .operation(async {
                Ok::<_, std::convert::Infallible>(
                    self.policy
                        .authorize_body(initial.initial, parts, bytes, state, route)
                        .await,
                )
            })
            .await
            .map_err(|_| EndpointError::Unavailable)??;
        Ok(context(
            initial,
            parts,
            state,
            route,
            &self.policy,
            &self.options,
        ))
    }
}
fn context<S: Clone + Send + Sync + 'static, P: EndpointPolicy<S>>(
    initial: P::Context,
    parts: &axum::http::request::Parts,
    state: &S,
    route: &RouteContract,
    policy: &P,
    options: &StreamOptions,
) -> StreamContext<P::Context> {
    let parts = parts.clone();
    let state = state.clone();
    let route = route.clone();
    let policy = policy.clone();
    let lifecycle = options.lifecycle.clone();
    let authorize = Arc::new(move || {
        let mut parts = parts.clone();
        let state = state.clone();
        let route = route.clone();
        let policy = policy.clone();
        let lifecycle = lifecycle.clone();
        Box::pin(async move {
            lifecycle
                .operation(async {
                    Ok::<_, std::convert::Infallible>(
                        policy.authorize(&mut parts, &state, &route).await,
                    )
                })
                .await
                .map_err(|_| EndpointError::Unavailable)?
        }) as AuthorizationFuture<P::Context>
    });
    StreamContext {
        initial,
        authorize,
        lifecycle: options.lifecycle.clone(),
    }
}
fn compile<S, C, I, T, F, Fut, Events>(
    route: &RouteContract,
    outputs: &[forklaunch_core::contract::ResponseContract],
    options: StreamOptions,
    handler: F,
) -> Result<BoundHandler<S, StreamContext<C>>, String>
where
    S: Clone + Send + Sync + 'static,
    C: Send + 'static,
    I: NativeBody,
    T: forklaunch_core::contract::__private::schemars::JsonSchema + Serialize + Send + 'static,
    F: Fn(State<S>, StreamContext<C>, I::Value) -> Fut + Clone + Send + Sync + 'static,
    Fut:
        Future<Output = Result<(Events, BTreeMap<String, String>), EndpointError>> + Send + 'static,
    Events: Stream<Item = Result<ServerEvent<T>, EndpointError>> + Send + 'static,
{
    if route.method == HttpMethod::Head
        || route.options.mcp
        || outputs.len() != 1
        || !(200..300).contains(&outputs[0].status_code)
        || matches!(outputs[0].status_code, 204 | 205)
    {
        return Err(
            "versioned SSE requires one body-bearing success representation and MCP disabled"
                .into(),
        );
    }
    let body = outputs[0].body.as_ref().ok_or("missing SSE body")?;
    let schema: serde_json::Value = serde_json::from_str(
        body.schema
            .json_schema
            .as_deref()
            .ok_or("missing SSE schema")?,
    )
    .map_err(|_| "invalid SSE schema")?;
    if schema["x-forklaunch-sse-framing"] != "payload" {
        return Err("native SSE requires explicit payload framing".into());
    }
    let identity = (outputs[0].status_code, "text/event-stream".to_owned());
    let selected = Arc::new(
        super::super::responses::version_plans(outputs)?
            .remove(&identity)
            .ok_or("missing SSE representation")?,
    );
    let request = Arc::new(RequestPlan::new(route)?);
    Ok(Arc::new(move |state, context, mut parts, bytes| {
        let options = options.clone();
        let handler = handler.clone();
        let request = request.clone();
        let selected = selected.clone();
        let identity = identity.clone();
        Box::pin(async move {
            let matched = parts
                .extensions
                .remove::<MatchedResponsePlans>()
                .ok_or(EndpointError::InvalidResponse)?;
            let input = request.decode::<I, S>(&mut parts, &state, &bytes).await?;
            let authorize = context.authorize.clone();
            let (events, header_values) = options
                .lifecycle
                .operation(async {
                    Ok::<_, std::convert::Infallible>(handler(State(state), context, input).await)
                })
                .await
                .map_err(|_| EndpointError::Unavailable)??;
            let header_response =
                matched.render_stream_headers(&selected, &identity, header_values)?;
            let status = header_response.status();
            let headers = header_response.headers().clone();
            let (sender, receiver) = tokio::sync::mpsc::channel(1);
            tokio::spawn(async move {
                let mut events = Box::pin(events);
                let mut refresh = tokio::time::interval(options.authorization_interval);
                refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tokio::select! {biased;
                        ()=options.shutdown.cancelled()=>break,
                        ()=sender.closed()=>break,
                        _=refresh.tick()=>{if authorize().await.is_err(){break;}},
                        event=events.next()=>{
                            let Some(Ok(event))=event else{break;};
                            if authorize().await.is_err(){break;}
                            let Ok(frame)=encode(event,&matched,&identity,options.max_message_bytes) else{break;};
                            if options.lifecycle.operation(sender.send(Ok::<_,std::convert::Infallible>(frame))).await.is_err(){break;}
                        }
                    }
                }
            });
            let stream = futures_util::stream::unfold(receiver, |mut receiver| async move {
                receiver.recv().await.map(|event| (event, receiver))
            });
            Ok((
                status,
                headers,
                Sse::new(stream).keep_alive(KeepAlive::default()),
            )
                .into_response())
        })
    }))
}
fn encode<T: Serialize>(
    event: ServerEvent<T>,
    matched: &MatchedResponsePlans,
    identity: &(u16, String),
    limit: usize,
) -> Result<axum::response::sse::Event, EndpointError> {
    let bytes = serde_json::to_vec(&event.data).map_err(|_| EndpointError::InvalidResponse)?;
    if bytes
        .len()
        .saturating_add(event.id.as_ref().map_or(0, String::len))
        .saturating_add(event.event.as_ref().map_or(0, String::len))
        > limit
    {
        return Err(EndpointError::InvalidResponse);
    }
    matched.validate_stream_data(identity, &bytes)?;
    let mut frame = axum::response::sse::Event::default()
        .data(String::from_utf8(bytes).map_err(|_| EndpointError::InvalidResponse)?);
    for value in [&event.id, &event.event].into_iter().flatten() {
        if value.contains(['\r', '\n', '\0']) {
            return Err(EndpointError::InvalidResponse);
        }
    }
    if let Some(id) = event.id {
        frame = frame.id(id);
    }
    if let Some(name) = event.event {
        frame = frame.event(name);
    }
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::Shutdown;
    use axum::{
        body::{Body, to_bytes},
        extract::Request,
        http::StatusCode,
    };
    use forklaunch_core::contract::{
        AccessLevel, ContractRegistry,
        endpoint::{Fields, NoBody},
    };
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;
    use tower::ServiceExt;
    #[derive(serde::Deserialize, schemars::JsonSchema)]
    struct OldQuery {
        #[schemars(regex(pattern = "^(both|first)$"))]
        select: String,
        emit: String,
    }
    #[derive(serde::Deserialize, schemars::JsonSchema)]
    struct NewQuery {
        #[schemars(regex(pattern = "^(both|last)$"))]
        select: String,
        emit: String,
    }
    #[derive(Serialize, schemars::JsonSchema)]
    struct OldEvent {
        #[schemars(regex(pattern = "^old$"))]
        value: String,
    }
    #[derive(Serialize, schemars::JsonSchema)]
    struct NewEvent {
        #[schemars(regex(pattern = "^new$"))]
        value: String,
    }
    #[derive(Clone)]
    struct Policy {
        allowed: Arc<AtomicBool>,
        body_checks: Arc<AtomicUsize>,
    }
    #[allow(clippy::unused_async_trait_impl)]
    impl EndpointPolicy<()> for Policy {
        type Context = ();
        async fn authorize(
            &self,
            _: &mut axum::http::request::Parts,
            (): &(),
            _: &RouteContract,
        ) -> Result<(), EndpointError> {
            if self.allowed.load(Ordering::SeqCst) {
                Ok(())
            } else {
                Err(EndpointError::Forbidden)
            }
        }
        async fn authorize_body(
            &self,
            (): (),
            _: &mut axum::http::request::Parts,
            _: &[u8],
            (): &(),
            _: &RouteContract,
        ) -> Result<(), EndpointError> {
            self.body_checks.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
    fn application(policy: Policy, pending: bool) -> axum::Router {
        let old = Endpoint::<Fields<NoBody, OldQuery>, SseBody<OldEvent>>::new(
            "Events",
            "Events",
            HttpMethod::Get,
            "/events",
            201,
        )
        .unwrap()
        .with_access(AccessLevel::Authenticated);
        let new = Endpoint::<Fields<NoBody, NewQuery>, SseBody<NewEvent>>::new(
            "Events",
            "Events",
            HttpMethod::Get,
            "/events",
            201,
        )
        .unwrap()
        .with_access(AccessLevel::Authenticated);
        let versions = VersionedEndpoint::new("first", &old)
            .unwrap()
            .with_version("last", &new)
            .unwrap();
        let mut registry = ContractRegistry::new();
        registry.register(versions.contract());
        let options = StreamOptions::new(
            Shutdown::default(),
            1024,
            Duration::from_millis(10),
            Duration::from_millis(40),
        )
        .unwrap();
        let handlers =
            VersionedSseHandlers::new(versions, options)
                .bind("first", old, move |_, _, input| async move {
                    let _ = input.query.select;
                    Ok(futures_util::stream::iter([Ok(ServerEvent::new(OldEvent {
                        value: input.query.emit,
                    }))])
                    .chain(
                        futures_util::stream::pending().take(if pending { usize::MAX } else { 0 }),
                    ))
                })
                .unwrap()
                .bind("last", new, move |_, _, input| async move {
                    let _ = input.query.select;
                    Ok(futures_util::stream::iter([Ok(ServerEvent::new(NewEvent {
                        value: input.query.emit,
                    }))])
                    .chain(
                        futures_util::stream::pending().take(if pending { usize::MAX } else { 0 }),
                    ))
                })
                .unwrap();
        ContractRouter::new(registry)
            .bind_versioned_sse_authorized(handlers, policy)
            .unwrap()
            .finish()
            .unwrap()
    }
    fn policy() -> Policy {
        Policy {
            allowed: Arc::new(AtomicBool::new(true)),
            body_checks: Arc::new(AtomicUsize::new(0)),
        }
    }
    #[derive(Serialize, schemars::JsonSchema)]
    struct OldHeaders {
        #[serde(rename = "x-mode")]
        #[schemars(regex(pattern = "^old$"))]
        mode: String,
    }
    #[derive(Serialize, schemars::JsonSchema)]
    struct NewHeaders {
        #[serde(rename = "x-mode")]
        #[schemars(regex(pattern = "^new$"))]
        mode: String,
    }
    #[tokio::test]
    async fn dynamic_headers_and_frames_match_independently() {
        for (header, expected) in [("new", 201), ("bad", 500)] {
            let old = Endpoint::<NoBody, SseBody<OldEvent>>::new(
                "Events",
                "Events",
                HttpMethod::Get,
                "/events",
                201,
            )
            .unwrap()
            .with_typed_response_headers::<OldHeaders>()
            .unwrap();
            let new = Endpoint::<NoBody, SseBody<NewEvent>>::new(
                "Events",
                "Events",
                HttpMethod::Get,
                "/events",
                201,
            )
            .unwrap()
            .with_typed_response_headers::<NewHeaders>()
            .unwrap();
            let versions = VersionedEndpoint::new("first", &old)
                .unwrap()
                .with_version("last", &new)
                .unwrap();
            let mut registry = ContractRegistry::new();
            registry.register(versions.contract());
            let options = StreamOptions::new(
                Shutdown::new(),
                1024,
                Duration::from_millis(10),
                Duration::from_millis(40),
            )
            .unwrap();
            let handlers = VersionedSseHandlers::new(versions, options)
                .bind_with_headers("first", old, |_, _, ()| async {
                    Ok(ResponseWithHeaders {
                        body: futures_util::stream::iter([Ok(ServerEvent::new(OldEvent {
                            value: "old".into(),
                        }))]),
                        headers: OldHeaders { mode: "old".into() },
                    })
                })
                .unwrap()
                .bind_with_headers("last", new, move |_, _, ()| async move {
                    Ok(ResponseWithHeaders {
                        body: futures_util::stream::iter([Ok(ServerEvent::new(NewEvent {
                            value: "old".into(),
                        }))]),
                        headers: NewHeaders {
                            mode: header.into(),
                        },
                    })
                })
                .unwrap();
            let app = ContractRouter::new(registry)
                .bind_versioned_sse_authorized(handlers, policy())
                .unwrap()
                .finish()
                .unwrap();
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/events")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), expected);
            if expected == 201 {
                assert_eq!(response.headers()["x-mode"], "new");
                let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
                assert!(String::from_utf8(bytes.to_vec()).unwrap().contains("old"));
            }
        }
    }
    #[tokio::test]
    async fn matched_frame_union_and_initial_security_status_are_preserved() {
        let policy = policy();
        let app = application(policy.clone(), false);
        for (select, emit, valid) in [
            ("both", "old", true),
            ("both", "new", true),
            ("first", "new", false),
            ("last", "old", false),
            ("both", "bad", false),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/events?select={select}&emit={emit}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::CREATED);
            let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
            assert_eq!(!bytes.is_empty(), valid, "{select}/{emit}");
        }
        assert_eq!(policy.body_checks.load(Ordering::SeqCst), 5);
        policy.allowed.store(false, Ordering::SeqCst);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/events?select=both&emit=old")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(policy.body_checks.load(Ordering::SeqCst), 5);
    }
    #[tokio::test]
    async fn idle_versioned_stream_closes_on_revocation() {
        let policy = policy();
        let app = application(policy.clone(), true);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/events?select=both&emit=old")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let mut bytes = response.into_body().into_data_stream();
        assert!(bytes.next().await.unwrap().is_ok());
        policy.allowed.store(false, Ordering::SeqCst);
        assert!(
            tokio::time::timeout(Duration::from_millis(200), bytes.next())
                .await
                .unwrap()
                .is_none()
        );
    }
}
