//! Static SSE binding delegates to the existing managed streaming admission path.
use crate::native::{
    ContractRouter, EndpointError, EndpointPolicy, NativeBody, ServerEvent, StreamContext,
    StreamOptions,
};
use axum::extract::State;
use forklaunch_core::contract::{
    bindings::{EndpointDefinition, MountBinding, SseBinding},
    endpoint::{EndpointBody, SseBody},
};
use futures_util::Stream;
use std::future::Future;

impl<S, D, P, H, F, Events, T> MountBinding<SseBinding<D, P, StreamOptions, H>>
    for ContractRouter<S>
where
    S: Clone + Send + Sync + 'static,
    D: EndpointDefinition<Output = SseBody<T>>,
    D::Input: NativeBody,
    T: forklaunch_core::contract::__private::schemars::JsonSchema
        + serde::Serialize
        + Send
        + 'static,
    P: EndpointPolicy<S>,
    H: Fn(State<S>, StreamContext<P::Context>, <D::Input as EndpointBody>::Value) -> F
        + Clone
        + Send
        + Sync
        + 'static,
    F: Future<Output = Result<Events, EndpointError>> + Send + 'static,
    Events: Stream<Item = Result<ServerEvent<T>, EndpointError>> + Send + 'static,
{
    fn mount_binding(self, binding: SseBinding<D, P, StreamOptions, H>) -> Result<Self, String> {
        let (policy, options, handler) = binding.into_parts();
        self.bind_sse_authorized(D::endpoint(), policy, options, handler)
    }
}

impl<S, D, P, H, F, Events, T, Headers>
    MountBinding<
        SseBinding<D, P, StreamOptions, H, forklaunch_core::contract::bindings::SseHeaders>,
    > for ContractRouter<S>
where
    S: Clone + Send + Sync + 'static,
    D: EndpointDefinition<
        Output = forklaunch_core::contract::endpoint::WithResponseHeaders<SseBody<T>, Headers>,
    >,
    D::Input: NativeBody,
    T: forklaunch_core::contract::__private::schemars::JsonSchema
        + serde::Serialize
        + Send
        + 'static,
    Headers: forklaunch_core::contract::__private::schemars::JsonSchema
        + serde::Serialize
        + Send
        + 'static,
    P: EndpointPolicy<S>,
    H: Fn(State<S>, StreamContext<P::Context>, <D::Input as EndpointBody>::Value) -> F
        + Clone
        + Send
        + Sync
        + 'static,
    F: Future<
            Output = Result<
                forklaunch_core::contract::endpoint::ResponseWithHeaders<Events, Headers>,
                EndpointError,
            >,
        > + Send
        + 'static,
    Events: Stream<Item = Result<ServerEvent<T>, EndpointError>> + Send + 'static,
{
    fn mount_binding(
        self,
        binding: SseBinding<
            D,
            P,
            StreamOptions,
            H,
            forklaunch_core::contract::bindings::SseHeaders,
        >,
    ) -> Result<Self, String> {
        let (policy, options, handler) = binding.into_parts();
        self.bind_sse_authorized_with_headers(D::endpoint(), policy, options, handler)
    }
}
