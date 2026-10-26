//! Typed mounting for host-generated complete operation sets.
//!
//! A handler returning the wrong response cannot mount:
//! ```compile_fail
//! use axum::extract::State;
//! use forklaunch_axum::native::ContractRouter;
//! use forklaunch_core::contract::{endpoint::{Endpoint, NoBody, TextBody}, HttpMethod};
//! #[forklaunch_core::contract::api]
//! mod operations {
//!     use super::*;
//!     pub fn hello() -> Endpoint<NoBody, TextBody> { Endpoint::new("hello", "hello", HttpMethod::Get, "/hello", 200).unwrap() }
//! }
//! let bindings = operations::Bindings {
//!     hello: operations::Hello::bind(|_: State<()>, ()| async { Ok(42_u32) }),
//! };
//! let _ = bindings.mount(ContractRouter::<()>::new(operations::registry()));
//! ```
//! A body-bearing operation cannot discard its typed input:
//! ```compile_fail
//! use axum::extract::State;
//! use forklaunch_axum::native::{ContractRouter, EndpointError};
//! use forklaunch_core::contract::{endpoint::{Endpoint, TextBody}, HttpMethod};
//! #[forklaunch_core::contract::api]
//! mod operations {
//!     use super::*;
//!     pub fn write() -> Endpoint<TextBody, TextBody> { Endpoint::new("write", "write", HttpMethod::Post, "/write", 200).unwrap() }
//! }
//! let bindings = operations::Bindings {
//!     write: operations::Write::bind(|_: State<()>| async { Ok::<_, EndpointError>("ignored".to_owned()) }),
//! };
//! let _ = bindings.mount(ContractRouter::<()>::new(operations::registry()));
//! ```
#[cfg(feature = "native-streams")]
mod sockets;
#[cfg(feature = "native-streams")]
mod streams;

use super::{ContractRouter, EndpointError, EndpointPolicy, NativeBody};
use axum::extract::State;
use forklaunch_core::contract::bindings::{
    AuthorizedBinding, EndpointDefinition, MountBinding, PublicBinding, PublicEndpointDefinition,
};
use forklaunch_core::contract::endpoint::EndpointBody;
use std::future::Future;

impl<S, D, H, F> MountBinding<PublicBinding<D, H>> for ContractRouter<S>
where
    S: Clone + Send + Sync + 'static,
    D: PublicEndpointDefinition,
    D::Input: NativeBody,
    D::Output: NativeBody,
    H: Fn(State<S>, <D::Input as EndpointBody>::Value) -> F + Clone + Send + Sync + 'static,
    F: Future<Output = Result<<D::Output as EndpointBody>::Value, EndpointError>> + Send + 'static,
{
    fn mount_binding(self, binding: PublicBinding<D, H>) -> Result<Self, String> {
        self.bind(D::endpoint(), binding.into_handler())
    }
}

// A distinct shape marker keeps ordinary two-argument callbacks compatible,
// while only actual NoBody contracts admit state-only handlers.
impl<S, D, H, F> MountBinding<PublicBinding<D, H>, (State<S>,)> for ContractRouter<S>
where
    S: Clone + Send + Sync + 'static,
    D: PublicEndpointDefinition<Input = forklaunch_core::contract::endpoint::NoBody>,
    D::Output: NativeBody,
    H: Fn(State<S>) -> F + Clone + Send + Sync + 'static,
    F: Future<Output = Result<<D::Output as EndpointBody>::Value, EndpointError>> + Send + 'static,
{
    fn mount_binding(self, binding: PublicBinding<D, H>) -> Result<Self, String> {
        let handler = binding.into_handler();
        self.bind(D::endpoint(), move |state, ()| handler(state))
    }
}

impl<S, D, P, H, F> MountBinding<AuthorizedBinding<D, P, H>> for ContractRouter<S>
where
    S: Clone + Send + Sync + 'static,
    D: EndpointDefinition,
    D::Input: NativeBody,
    D::Output: NativeBody,
    P: EndpointPolicy<S>,
    H: Fn(State<S>, P::Context, <D::Input as EndpointBody>::Value) -> F
        + Clone
        + Send
        + Sync
        + 'static,
    F: Future<Output = Result<<D::Output as EndpointBody>::Value, EndpointError>> + Send + 'static,
{
    fn mount_binding(self, binding: AuthorizedBinding<D, P, H>) -> Result<Self, String> {
        let (policy, handler) = binding.into_parts();
        self.bind_authorized(D::endpoint(), policy, handler)
    }
}

impl<S, D, P, H, F> MountBinding<AuthorizedBinding<D, P, H>, (State<S>, P::Context)>
    for ContractRouter<S>
where
    S: Clone + Send + Sync + 'static,
    D: EndpointDefinition<Input = forklaunch_core::contract::endpoint::NoBody>,
    D::Output: NativeBody,
    P: EndpointPolicy<S>,
    H: Fn(State<S>, P::Context) -> F + Clone + Send + Sync + 'static,
    F: Future<Output = Result<<D::Output as EndpointBody>::Value, EndpointError>> + Send + 'static,
{
    fn mount_binding(self, binding: AuthorizedBinding<D, P, H>) -> Result<Self, String> {
        let (policy, handler) = binding.into_parts();
        self.bind_authorized(D::endpoint(), policy, move |state, context, ()| {
            handler(state, context)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use forklaunch_core::contract::{
        HttpMethod,
        endpoint::{Endpoint, NoBody, TextBody},
    };
    use tower::ServiceExt;

    #[forklaunch_core::contract::api]
    mod operations {
        use super::*;
        pub fn hello() -> Endpoint<NoBody, TextBody> {
            Endpoint::new("hello", "hello", HttpMethod::Get, "/hello", 200).unwrap()
        }
    }

    #[tokio::test]
    async fn no_body_authorized_handler_receives_verified_context_and_denial_precedes_handler() {
        #[forklaunch_core::contract::api]
        mod secured {
            use super::*;
            #[access(Authenticated)]
            pub fn hello() -> Endpoint<NoBody, TextBody> {
                Endpoint::new("hello", "hello", HttpMethod::Get, "/hello", 200).unwrap()
            }
        }
        #[derive(Clone)]
        struct Verify;
        impl EndpointPolicy<()> for Verify {
            type Context = String;
            fn authorize(
                &self,
                parts: &mut axum::http::request::Parts,
                (): &(),
                _: &forklaunch_core::contract::RouteContract,
            ) -> impl Future<Output = Result<String, EndpointError>> + Send {
                std::future::ready(
                    if parts
                        .headers
                        .get("x-fixture-key")
                        .is_some_and(|value| value == "valid")
                    {
                        Ok("verified".into())
                    } else {
                        Err(EndpointError::Unauthorized)
                    },
                )
            }
        }
        let router = secured::Bindings {
            hello: secured::Hello::bind_authorized(
                Verify,
                |_: State<()>, identity: String| async move { Ok(identity) },
            ),
        }
        .mount(ContractRouter::new(secured::registry()))
        .unwrap()
        .finish()
        .unwrap();
        let denied = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/hello")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(denied.status(), 401);
        let allowed = router
            .oneshot(
                Request::builder()
                    .uri("/hello")
                    .header("x-fixture-key", "valid")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(allowed.status(), 200);
        assert_eq!(
            to_bytes(allowed.into_body(), 100).await.unwrap(),
            "verified"
        );
    }

    #[tokio::test]
    async fn state_only_no_body_handler_uses_static_bindings_and_real_state() {
        async fn hello(State(message): State<String>) -> Result<String, EndpointError> {
            Ok(message)
        }
        let router = operations::Bindings {
            hello: operations::Hello::bind(hello),
        }
        .mount(ContractRouter::new(operations::registry()))
        .unwrap()
        .finish()
        .unwrap()
        .with_state("state-only".to_owned());
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/hello")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            to_bytes(response.into_body(), 100).await.unwrap(),
            "state-only"
        );
    }

    #[tokio::test]
    async fn complete_generated_bindings_use_the_existing_native_validation_path() {
        let bindings = operations::Bindings {
            hello: operations::Hello::bind(|_: State<()>, ()| async { Ok("hello".to_owned()) }),
        };
        let router = bindings
            .mount(ContractRouter::new(operations::registry()))
            .unwrap()
            .finish()
            .unwrap();
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/hello")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(to_bytes(response.into_body(), 100).await.unwrap(), "hello");
    }
}
