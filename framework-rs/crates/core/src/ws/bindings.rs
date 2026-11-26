//! Static socket identities and complete binding sets, independent of runtime transport.
//!
//! Omitting a declared socket is a compilation error:
//! ```compile_fail
//! use forklaunch_core::{contract::{RouteContract, endpoint::NoBody}, ws::TypedSocket};
//! #[forklaunch_core::ws::socket_api]
//! mod sockets {
//!     use super::*;
//!     pub fn first() -> TypedSocket<NoBody, String, String> { TypedSocket::new(RouteContract::get("first","first","/first"),"in","out").unwrap() }
//!     pub fn second() -> TypedSocket<NoBody, String, String> { TypedSocket::new(RouteContract::get("second","second","/second"),"in","out").unwrap() }
//! }
//! let _ = sockets::Bindings { first: sockets::First::bind_authorized((), (), ()) };
//! ```
//! Identical message types do not make different sockets interchangeable:
//! ```compile_fail
//! use forklaunch_core::{contract::{RouteContract, endpoint::NoBody}, ws::TypedSocket};
//! #[forklaunch_core::ws::socket_api]
//! mod sockets {
//!     use super::*;
//!     pub fn first() -> TypedSocket<NoBody, String, String> { TypedSocket::new(RouteContract::get("first","first","/first"),"in","out").unwrap() }
//!     pub fn second() -> TypedSocket<NoBody, String, String> { TypedSocket::new(RouteContract::get("second","second","/second"),"in","out").unwrap() }
//! }
//! let _ = sockets::Bindings { first: sockets::Second::bind_authorized((), (), ()), second: sockets::First::bind_authorized((), (), ()) };
//! ```
use super::TypedSocket;
use crate::contract::{__private::schemars::JsonSchema, endpoint::EndpointBody};
use std::marker::PhantomData;

pub trait SocketDefinition: Sized + 'static {
    type Input: EndpointBody;
    type Incoming: JsonSchema;
    type Outgoing: JsonSchema;
    const NAME: &'static str;
    fn socket() -> TypedSocket<Self::Input, Self::Incoming, Self::Outgoing>;
}
mod sealed {
    pub trait Sealed {}
}
/// Compiler-resolved descriptor types, including ordinary aliases.
pub trait SocketDescriptor: sealed::Sealed {
    type Input: EndpointBody;
    type Incoming: JsonSchema;
    type Outgoing: JsonSchema;
    fn into_socket(self) -> TypedSocket<Self::Input, Self::Incoming, Self::Outgoing>;
}
impl<I, In, Out> sealed::Sealed for TypedSocket<I, In, Out> {}
impl<I: EndpointBody, In: JsonSchema, Out: JsonSchema> SocketDescriptor
    for TypedSocket<I, In, Out>
{
    type Input = I;
    type Incoming = In;
    type Outgoing = Out;
    fn into_socket(self) -> TypedSocket<I, In, Out> {
        self
    }
}
pub trait SocketOperationBinding: sealed::Sealed {
    type Definition: SocketDefinition;
}
/// Explicit policy and bounded runtime configuration remain required for every socket.
pub struct AuthorizedSocketBinding<D, P, C, H> {
    policy: P,
    configuration: C,
    handler: H,
    definition: PhantomData<fn() -> D>,
}
impl<D: SocketDefinition, P, C, H> AuthorizedSocketBinding<D, P, C, H> {
    #[must_use]
    pub fn new(policy: P, configuration: C, handler: H) -> Self {
        Self {
            policy,
            configuration,
            handler,
            definition: PhantomData,
        }
    }
    pub fn into_parts(self) -> (P, C, H) {
        (self.policy, self.configuration, self.handler)
    }
}
impl<D, P, C, H> sealed::Sealed for AuthorizedSocketBinding<D, P, C, H> {}
impl<D: SocketDefinition, P, C, H> SocketOperationBinding for AuthorizedSocketBinding<D, P, C, H> {
    type Definition = D;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{RouteContract, bindings::MountBinding, endpoint::NoBody};
    #[crate::ws::socket_api]
    mod sockets {
        use super::*;
        type Socket = TypedSocket<NoBody, String, String>;
        pub fn first() -> Socket {
            TypedSocket::new(RouteContract::get("first", "first", "/first"), "in", "out").unwrap()
        }
        pub fn second() -> Socket {
            TypedSocket::new(
                RouteContract::get("second", "second", "/second"),
                "in",
                "out",
            )
            .unwrap()
        }
    }
    struct Recorder(Vec<&'static str>);
    impl<B: SocketOperationBinding> MountBinding<B> for Recorder {
        fn mount_binding(mut self, _: B) -> Result<Self, String> {
            self.0.push(B::Definition::NAME);
            Ok(self)
        }
    }
    #[test]
    fn complete_socket_set_resolves_aliases_and_keeps_finite_registry_empty() {
        let registry = sockets::registry().unwrap();
        assert_eq!(registry.socket_contracts().len(), 2);
        assert!(registry.routes().is_empty());
        let bindings = sockets::Bindings {
            first: sockets::First::bind_authorized((), (), ()),
            second: sockets::Second::bind_authorized((), (), ()),
        };
        assert_eq!(
            bindings.mount(Recorder(Vec::new())).unwrap().0,
            ["first", "second"]
        );
    }
}
