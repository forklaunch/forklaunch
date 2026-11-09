//! Typed stream registration with bounded work and periodic authorization.
use super::{ContractRouter, EndpointError, EndpointPolicy, NativeBody, RequestPlan};
use crate::native::{JsonMessage, Shutdown, StreamLifecycle};
use axum::{
    body::to_bytes,
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response, Sse, sse::KeepAlive},
    routing::{MethodFilter, get, on},
};
use forklaunch_core::contract::{
    HttpMethod, RouteContract,
    endpoint::{Endpoint, EndpointBody, ResponseWithHeaders, SseBody, WithResponseHeaders},
};
use futures_util::{Stream, StreamExt};
use serde::Serialize;
use std::{collections::BTreeMap, future::Future, sync::Arc, time::Duration};

#[derive(Clone)]
pub struct StreamOptions {
    shutdown: Shutdown,
    max_message_bytes: usize,
    authorization_interval: Duration,
    pub(super) lifecycle: StreamLifecycle,
}
impl StreamOptions {
    /// # Errors
    /// Requires 1..=1MiB frames and nonzero authorization/operation bounds <=30s.
    pub fn new(
        shutdown: Shutdown,
        max_message_bytes: usize,
        authorization_interval: Duration,
        operation_timeout: Duration,
    ) -> Result<Self, String> {
        if max_message_bytes == 0
            || max_message_bytes > 1024 * 1024
            || authorization_interval.is_zero()
            || authorization_interval > Duration::from_secs(30)
        {
            return Err("invalid stream bounds".into());
        }
        let lifecycle = StreamLifecycle::new(shutdown.clone(), operation_timeout)
            .map_err(|_| "invalid stream operation timeout")?;
        Ok(Self {
            shutdown,
            max_message_bytes,
            authorization_interval,
            lifecycle,
        })
    }
}
type AuthorizationFuture<C> =
    std::pin::Pin<Box<dyn Future<Output = Result<C, EndpointError>> + Send>>;
type Authorization<C> = Arc<dyn Fn() -> AuthorizationFuture<C> + Send + Sync>;

/// Verified initial context and an opaque capability to refresh it before storage work.
pub struct StreamContext<C> {
    initial: C,
    lifecycle: StreamLifecycle,
    authorize: Authorization<C>,
}
impl<C> StreamContext<C> {
    #[must_use]
    pub fn initial(&self) -> &C {
        &self.initial
    }
    /// # Errors
    /// Rejects revoked credentials, authorization failure, shutdown or timeout.
    pub async fn refresh(&self) -> Result<C, EndpointError> {
        (self.authorize)().await
    }
    /// # Errors
    /// Rejects application failure, timeout or cancellation.
    pub async fn operation<T, E>(
        &self,
        work: impl Future<Output = Result<T, E>>,
    ) -> Result<T, EndpointError> {
        self.lifecycle
            .operation(work)
            .await
            .map_err(|_| EndpointError::Unavailable)
    }
}
/// Only data is application-defined; the adapter validates control fields and JSON.
pub struct ServerEvent<T> {
    pub id: Option<String>,
    pub event: Option<String>,
    pub data: T,
}
impl<T> ServerEvent<T> {
    #[must_use]
    pub fn new(data: T) -> Self {
        Self {
            id: None,
            event: None,
            data,
        }
    }
    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
    #[must_use]
    pub fn with_event(mut self, event: impl Into<String>) -> Self {
        self.event = Some(event.into());
        self
    }
    fn encode(
        &self,
        limit: usize,
        plan: &super::responses::ResponsePlan,
    ) -> Result<axum::response::sse::Event, EndpointError>
    where
        T: Serialize,
    {
        let bytes = serde_json::to_vec(&self.data).map_err(|_| EndpointError::InvalidResponse)?;
        if bytes
            .len()
            .saturating_add(self.id.as_ref().map_or(0, String::len))
            .saturating_add(self.event.as_ref().map_or(0, String::len))
            > limit
        {
            return Err(EndpointError::InvalidResponse);
        }
        plan.validate_data(&bytes)?;
        let data = String::from_utf8(bytes).map_err(|_| EndpointError::InvalidResponse)?;
        let mut output = axum::response::sse::Event::default().data(data);
        for value in [&self.id, &self.event].into_iter().flatten() {
            if value.contains(['\r', '\n', '\0']) {
                return Err(EndpointError::InvalidResponse);
            }
        }
        if let Some(id) = &self.id {
            output = output.id(id);
        }
        if let Some(event) = &self.event {
            output = output.event(event);
        }
        Ok(output)
    }
}
impl<S: Clone + Send + Sync + 'static> ContractRouter<S> {
    /// Bind a typed SSE producer with periodic authorization, bounded backpressure,
    /// cancellation, and per-frame validation against the published event schema.
    /// # Errors
    /// Rejects missing/divergent bindings, unsupported headers, HEAD routes and
    /// statuses that cannot carry a successful event-stream body.
    pub fn bind_sse_authorized<I, T, P, F, Fut, Events>(
        self,
        endpoint: Endpoint<I, SseBody<T>>,
        policy: P,
        options: StreamOptions,
        handler: F,
    ) -> Result<Self, String>
    where
        I: NativeBody,
        T: forklaunch_core::contract::__private::schemars::JsonSchema + Serialize + Send + 'static,
        P: EndpointPolicy<S>,
        F: Fn(State<S>, StreamContext<P::Context>, I::Value) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<Events, EndpointError>> + Send + 'static,
        Events: Stream<Item = Result<ServerEvent<T>, EndpointError>> + Send + 'static,
    {
        self.bind_sse_inner(endpoint, policy, options, move |state, context, input| {
            let future = handler(state, context, input);
            async move { Ok((future.await?, BTreeMap::new())) }
        })
    }
    /// Bind an SSE producer with the same typed response-header wrapper used by
    /// finite endpoints. Headers are checked before the stream starts.
    /// # Errors
    /// Rejects divergent contracts, invalid header schemas or unsupported streams.
    pub fn bind_sse_authorized_with_headers<I, T, H, P, F, Fut, Events>(
        self,
        endpoint: Endpoint<I, WithResponseHeaders<SseBody<T>, H>>,
        policy: P,
        options: StreamOptions,
        handler: F,
    ) -> Result<Self, String>
    where
        I: NativeBody,
        T: forklaunch_core::contract::__private::schemars::JsonSchema + Serialize + Send + 'static,
        H: forklaunch_core::contract::__private::schemars::JsonSchema + Serialize + Send + 'static,
        P: EndpointPolicy<S>,
        F: Fn(State<S>, StreamContext<P::Context>, I::Value) -> Fut + Clone + Send + Sync + 'static,
        Fut:
            Future<Output = Result<ResponseWithHeaders<Events, H>, EndpointError>> + Send + 'static,
        Events: Stream<Item = Result<ServerEvent<T>, EndpointError>> + Send + 'static,
    {
        self.bind_sse_inner(endpoint, policy, options, move |state, context, input| {
            let future = handler(state, context, input);
            async move {
                let response = future.await?;
                let headers =
                    forklaunch_core::contract::endpoint::encode_response_headers(response.headers)
                        .map_err(|_| EndpointError::InvalidResponse)?;
                Ok((response.body, headers))
            }
        })
    }
    #[allow(clippy::too_many_lines)]
    fn bind_sse_inner<I, O, T, P, F, Fut, Events>(
        mut self,
        endpoint: Endpoint<I, O>,
        policy: P,
        options: StreamOptions,
        handler: F,
    ) -> Result<Self, String>
    where
        I: NativeBody,
        O: EndpointBody,
        T: forklaunch_core::contract::__private::schemars::JsonSchema + Serialize + Send + 'static,
        P: EndpointPolicy<S>,
        F: Fn(State<S>, StreamContext<P::Context>, I::Value) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<(Events, BTreeMap<String, String>), EndpointError>>
            + Send
            + 'static,
        Events: Stream<Item = Result<ServerEvent<T>, EndpointError>> + Send + 'static,
    {
        let outputs = endpoint.output_contracts();
        let route = endpoint.into_contract();
        P::CAPABILITIES.admit(&route)?;
        let index = self.admit_stream_route(&route)?;
        if route.method == HttpMethod::Head
            || route.options.mcp
            || outputs.len() != 1
            || !(200..300).contains(&outputs[0].status_code)
            || matches!(outputs[0].status_code, 204 | 205)
        {
            return Err(
                "SSE requires one body-bearing success representation and cannot use HEAD".into(),
            );
        }
        let status = StatusCode::from_u16(outputs[0].status_code)
            .map_err(|_| "invalid SSE response status")?;
        let method: axum::http::Method = route
            .method
            .as_str()
            .parse()
            .map_err(|_| "unsupported SSE method")?;
        let filter = MethodFilter::try_from(method).map_err(|_| "unsupported SSE method")?;
        let schema = outputs[0]
            .body
            .as_ref()
            .and_then(|body| body.schema.json_schema.as_ref())
            .ok_or("missing SSE schema")?;
        let mut payload: serde_json::Value =
            serde_json::from_str(schema).map_err(|_| "invalid SSE schema")?;
        // The compiler annotates native SSE payloads so SDKs can construct event
        // envelopes. Framing is transport metadata, not part of the JSON data.
        // Remove only this recognized annotation; retain exact payload matching.
        if payload
            .get("x-forklaunch-sse-framing")
            .and_then(serde_json::Value::as_str)
            != Some("payload")
        {
            return Err("native SSE requires explicit payload framing".into());
        }
        payload
            .as_object_mut()
            .ok_or("invalid SSE payload schema")?
            .remove("x-forklaunch-sse-framing");
        let _ = JsonMessage::<T>::new(&payload, options.max_message_bytes)?;
        let data_plan = Arc::new(
            super::responses::configure(super::responses::version_plans(&outputs)?, &route)
                .into_values()
                .next()
                .ok_or("missing SSE response")?,
        );
        // Reuse finite-response header validation without buffering stream data.
        let mut header_contract = outputs[0].clone();
        header_contract.body = None;
        let header_plan = Arc::new(
            super::responses::configure(super::responses::plans(&[header_contract])?, &route)
                .into_values()
                .next()
                .ok_or("missing SSE headers")?,
        );
        let request_plan = Arc::new(RequestPlan::new(&route)?);
        let path = route.openapi_path.clone();
        let route = Arc::new(route);
        self.router=self.router.route(&path,on(filter,move|State(state):State<S>,request:Request|{
            let policy=policy.clone();let options=options.clone();let handler=handler.clone();let route=route.clone();let request_plan=request_plan.clone();let header_plan=header_plan.clone();let data_plan=data_plan.clone();
            async move {
                let (mut parts,body)=request.into_parts();
                let context=options.lifecycle.operation(async{Ok::<_,std::convert::Infallible>(policy.authorize(&mut parts,&state,&route).await)}).await.map_err(|_|EndpointError::Unavailable)??;
                let bytes=options.lifecycle.operation(to_bytes(body,2*1024*1024)).await.map_err(|_|EndpointError::PayloadTooLarge)?;
                let context=options.lifecycle.operation(async{Ok::<_,std::convert::Infallible>(policy.authorize_body(context,&mut parts,&bytes,&state,&route).await)}).await.map_err(|_|EndpointError::Unavailable)??;
                let input=request_plan.decode::<I,S>(&mut parts,&state,&bytes).await?;
                let authorize={
                    let policy=policy.clone();let parts=parts.clone();let state=state.clone();let route=route.clone();let lifecycle=options.lifecycle.clone();
                    Arc::new(move||{
                        let policy=policy.clone();let mut parts=parts.clone();let state=state.clone();let route=route.clone();let lifecycle=lifecycle.clone();
                        Box::pin(async move{lifecycle.operation(async{Ok::<_,std::convert::Infallible>(policy.authorize(&mut parts,&state,&route).await)}).await.map_err(|_|EndpointError::Unavailable)?}) as AuthorizationFuture<P::Context>
                    })
                };
                let context=StreamContext{initial:context,authorize,lifecycle:options.lifecycle.clone()};
                let (events,header_values)=options.lifecycle.operation(async{Ok::<_,std::convert::Infallible>(handler(State(state.clone()),context,input).await)}).await.map_err(|_|EndpointError::Unavailable)??;
                let headers=header_plan.render(Vec::new(),header_values)?.into_parts().0.headers;
                let (sender,receiver)=tokio::sync::mpsc::channel(1);
                tokio::spawn(async move {
                    let mut events=Box::pin(events);
                    let mut refresh=tokio::time::interval(options.authorization_interval);
                    refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    loop {
                        tokio::select! { biased;
                            ()=options.shutdown.cancelled()=>break,
                            ()=sender.closed()=>break,
                            _=refresh.tick()=>{
                                if options.lifecycle.operation(policy.authorize(&mut parts,&state,&route)).await.is_err(){break;}
                            }
                            event=events.next()=>{
                                let Some(Ok(event))=event else{break;};
                                if options.lifecycle.operation(policy.authorize(&mut parts,&state,&route)).await.is_err(){break;}
                                let Ok(event)=event.encode(options.max_message_bytes,&data_plan) else{break;};
                                if options.lifecycle.operation(sender.send(Ok::<_,std::convert::Infallible>(event))).await.is_err(){break;}
                            }
                        }
                    }
                });
                let stream=futures_util::stream::unfold(receiver,|mut receiver|async move{receiver.recv().await.map(|event|(event,receiver))});
                Ok::<Response,EndpointError>((status,headers,Sse::new(stream).keep_alive(KeepAlive::default())).into_response())
            }
        }));
        self.mounted.push(index);
        Ok(self)
    }
    fn admit_stream_route(&self, route: &RouteContract) -> Result<usize, String> {
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
            || candidates[0].1 != route
            || self.mounted.contains(&candidates[0].0)
        {
            return Err(
                "stream endpoint differs from published contract or is already bound".into(),
            );
        }
        Ok(candidates[0].0)
    }
}

/// Application messages carry freshly verified context. Socket reads/writes,
/// schema checks and refresh timers remain in the framework task.
pub struct SocketSession<C, In, Out> {
    incoming: tokio::sync::mpsc::Receiver<(C, In)>,
    outgoing: tokio::sync::mpsc::Sender<(
        Out,
        tokio::sync::oneshot::Sender<Result<(), crate::native::StreamClosed>>,
    )>,
    lifecycle: StreamLifecycle,
}
impl<C, In, Out> SocketSession<C, In, Out> {
    pub async fn recv(&mut self) -> Option<(C, In)> {
        self.incoming.recv().await
    }
    /// # Errors
    /// Closes on application failure, timeout or shutdown.
    pub async fn operation<T, E>(
        &self,
        work: impl Future<Output = Result<T, E>>,
    ) -> Result<T, crate::native::StreamClosed> {
        self.lifecycle.operation(work).await
    }
    /// # Errors
    /// Closes on cancellation, backpressure, revoked authorization, invalid output or network failure.
    pub async fn send(&self, value: Out) -> Result<(), crate::native::StreamClosed> {
        let (ack, done) = tokio::sync::oneshot::channel();
        self.lifecycle
            .operation(self.outgoing.send((value, ack)))
            .await?;
        self.lifecycle.operation(done).await?
    }
}
impl<S: Clone + Send + Sync + 'static> ContractRouter<S> {
    /// Bind one registered socket with typed JSON messages and handshake metadata.
    /// Authentication refresh runs independently of application progress; every
    /// delivered/sent application message is preceded by a fresh policy check.
    /// # Errors
    /// Rejects missing, duplicate or drifting socket metadata and unbounded options.
    #[allow(clippy::too_many_lines)]
    pub fn bind_socket_authorized<I, In, Out, P, F, Fut>(
        mut self,
        name: &str,
        endpoint: forklaunch_ws::TypedSocket<I, In, Out>,
        policy: P,
        options: StreamOptions,
        handler: F,
    ) -> Result<Self, String>
    where
        I: NativeBody,
        In: forklaunch_core::contract::__private::schemars::JsonSchema
            + serde::de::DeserializeOwned
            + Send
            + 'static,
        Out:
            forklaunch_core::contract::__private::schemars::JsonSchema + Serialize + Send + 'static,
        P: EndpointPolicy<S>,
        F: Fn(State<S>, P::Context, I::Value, SocketSession<P::Context, In, Out>) -> Fut
            + Clone
            + Send
            + Sync
            + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        use axum::extract::{FromRequestParts, WebSocketUpgrade, ws::Message};
        use futures_util::SinkExt;
        let (route, socket) = endpoint.into_parts();
        P::CAPABILITIES.admit(&route)?;
        let expected = serde_json::to_value(&socket).map_err(|_| "invalid socket contract")?;
        let published = self
            .registry
            .socket_contracts()
            .get(name)
            .ok_or("socket absent from published contract")?;
        let published: serde_json::Value =
            serde_json::from_str(published).map_err(|_| "invalid published socket")?;
        if expected != published
            || self.mounted_sockets.iter().any(|mounted| mounted == name)
            || self.registry.routes().iter().any(|candidate| {
                candidate.method == HttpMethod::Get && candidate.path == route.path
            })
        {
            return Err("socket differs from published contract, is duplicated or conflicts with HTTP route".into());
        }
        let incoming = JsonMessage::<In>::new(
            &forklaunch_core::contract::schema_value_for::<In>(),
            options.max_message_bytes,
        )?;
        let outgoing = JsonMessage::<Out>::new(
            &forklaunch_core::contract::schema_value_for::<Out>(),
            options.max_message_bytes,
        )?;
        let plan = Arc::new(RequestPlan::new(&route)?);
        let path = route.openapi_path.clone();
        let route = Arc::new(route);
        self.router=self.router.route(&path,get(move|State(state):State<S>,request:Request|{
            let policy=policy.clone();let options=options.clone();let handler=handler.clone();let route=route.clone();let plan=plan.clone();let incoming=incoming.clone();let outgoing=outgoing.clone();
            async move {
                let (mut parts,body)=request.into_parts();
                let context=options.lifecycle.operation(async{Ok::<_,std::convert::Infallible>(policy.authorize(&mut parts,&state,&route).await)}).await.map_err(|_|EndpointError::Unavailable)??;
                let bytes=options.lifecycle.operation(to_bytes(body,2*1024*1024)).await.map_err(|_|EndpointError::PayloadTooLarge)?;
                let context=options.lifecycle.operation(async{Ok::<_,std::convert::Infallible>(policy.authorize_body(context,&mut parts,&bytes,&state,&route).await)}).await.map_err(|_|EndpointError::Unavailable)??;
                let input=plan.decode::<I,S>(&mut parts,&state,&bytes).await?;
                let upgrade=WebSocketUpgrade::from_request_parts(&mut parts,&state).await.map_err(|_|EndpointError::InvalidRequest)?;
                Ok::<Response,EndpointError>(upgrade.max_message_size(options.max_message_bytes).max_frame_size(options.max_message_bytes)
                    .on_upgrade(move|socket|async move{
                        let (to_application,from_socket)=tokio::sync::mpsc::channel(1);
                        let (to_socket,mut from_application)=tokio::sync::mpsc::channel::<(Out,tokio::sync::oneshot::Sender<Result<(),crate::native::StreamClosed>>)>(1);
                        let session=SocketSession{incoming:from_socket,outgoing:to_socket,lifecycle:options.lifecycle.clone()};
                        let application=handler(State(state.clone()),context,input,session);
                        let io=async {
                            let (mut writer,mut reader)=socket.split();
                            let mut refresh=tokio::time::interval(options.authorization_interval);
                            refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                            loop {
                                tokio::select!{biased;
                                    ()=options.shutdown.cancelled()=>break,
                                    ()=to_application.closed()=>break,
                                    _=refresh.tick()=>if options.lifecycle.operation(policy.authorize(&mut parts,&state,&route)).await.is_err(){break;},
                                    value=from_application.recv()=>{
                                        let Some((value,ack))=value else{break;};
                                        if options.lifecycle.operation(policy.authorize(&mut parts,&state,&route)).await.is_err(){break;}
                                        let Ok(json)=outgoing.encode(&value) else{break;};
                                        let result=options.lifecycle.operation(writer.send(Message::Text(json.into()))).await;
                                        let failed=result.is_err();let _=ack.send(result);if failed{break;}
                                    }
                                    frame=reader.next()=>match frame {
                                        Some(Ok(frame @ (Message::Text(_) | Message::Binary(_))))=>{
                                            let bytes=match frame {Message::Text(text)=>text.as_bytes().to_vec(),Message::Binary(bytes)=>bytes.to_vec(),_=>unreachable!("matched application frame")};
                                            let Ok(context)=options.lifecycle.operation(policy.authorize(&mut parts,&state,&route)).await else{break;};
                                            let Ok(value)=incoming.decode(&bytes) else{break;};
                                            if options.lifecycle.operation(to_application.send((context,value))).await.is_err(){break;}
                                        }
                                        Some(Ok(Message::Ping(bytes)))=>if options.lifecycle.operation(writer.send(Message::Pong(bytes))).await.is_err(){break;},
                                        Some(Ok(Message::Pong(_)))=>{},
                                        _=>break,
                                    }
                                }
                            }
                            let _=options.lifecycle.operation(writer.close()).await;
                        };
                        tokio::select!{biased;()=options.shutdown.cancelled()=>{},()=io=>{},()=application=>{}}
                    }).into_response())
            }
        }));
        self.mounted_sockets.push(name.into());
        Ok(self)
    }
}

#[cfg(test)]
mod tests;

mod versions;
pub use versions::VersionedSseHandlers;

#[cfg(test)]
mod static_sockets_tests;
