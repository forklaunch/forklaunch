//! Static socket mounting reuses the existing managed handshake and message path.
//!
//! A handler with the wrong incoming message type cannot mount:
//! ```compile_fail
//! use axum::extract::State;
//! use forklaunch_axum::native::{ContractRouter, EndpointPolicy, SocketSession, StreamOptions};
//! use forklaunch_core::{contract::{RouteContract, endpoint::NoBody}, ws::TypedSocket};
//! #[forklaunch_core::ws::socket_api]
//! mod sockets {
//!     use super::*;
//!     pub fn chat() -> TypedSocket<NoBody, String, String> { TypedSocket::new(RouteContract::get("chat","chat","/chat"),"in","out").unwrap() }
//! }
//! fn bind<P: EndpointPolicy<()>>(policy:P,options:StreamOptions) {
//!     let bindings=sockets::Bindings { chat:sockets::Chat::bind_authorized(policy,options,
//!         |_:State<()>, _:P::Context, (), _:SocketSession<P::Context,u32,String>|async {}) };
//!     let _=bindings.mount(ContractRouter::<()>::new(sockets::registry().unwrap()));
//! }
//! ```
use crate::native::{ContractRouter, EndpointPolicy, NativeBody, SocketSession, StreamOptions};
use axum::extract::State;
use forklaunch_core::{
    contract::{bindings::MountBinding, endpoint::EndpointBody},
    ws::bindings::{AuthorizedSocketBinding, SocketDefinition},
};
use std::future::Future;

impl<S, D, P, H, F> MountBinding<AuthorizedSocketBinding<D, P, StreamOptions, H>>
    for ContractRouter<S>
where
    S: Clone + Send + Sync + 'static,
    D: SocketDefinition,
    D::Input: NativeBody,
    D::Incoming: serde::de::DeserializeOwned + Send + 'static,
    D::Outgoing: serde::Serialize + Send + 'static,
    P: EndpointPolicy<S>,
    H: Fn(
            State<S>,
            P::Context,
            <D::Input as EndpointBody>::Value,
            SocketSession<P::Context, D::Incoming, D::Outgoing>,
        ) -> F
        + Clone
        + Send
        + Sync
        + 'static,
    F: Future<Output = ()> + Send + 'static,
{
    fn mount_binding(
        self,
        binding: AuthorizedSocketBinding<D, P, StreamOptions, H>,
    ) -> Result<Self, String> {
        let (policy, options, handler) = binding.into_parts();
        self.bind_socket_authorized(D::NAME, D::socket(), policy, options, handler)
    }
}
