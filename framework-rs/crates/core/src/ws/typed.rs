//! Host-compilable typed socket descriptions shared by SDK builds and native bindings.
use crate::contract::{
    HttpMethod, RouteContract,
    endpoint::{Endpoint, EndpointBody, NoBody},
};
use crate::ws::{SocketContract, SocketSchemaEntry};
use std::marker::PhantomData;

pub struct TypedSocket<Request, Incoming, Outgoing> {
    route: RouteContract,
    socket: SocketContract,
    marker: PhantomData<fn(Request, Incoming) -> Outgoing>,
}
impl<I, In, Out> TypedSocket<I, In, Out>
where
    I: EndpointBody,
    In: crate::contract::__private::schemars::JsonSchema,
    Out: crate::contract::__private::schemars::JsonSchema,
{
    /// Native JSON socket messages use the same input/output structs as generated SDKs.
    /// # Errors
    /// Requires an unversioned GET handshake without a request body or responses;
    /// typed path/query/header declarations must match the published handshake.
    pub fn new(
        mut route: RouteContract,
        client_message: &str,
        server_message: &str,
    ) -> Result<Self, &'static str> {
        if route.method != HttpMethod::Get
            || !route.responses.is_empty()
            || I::contract().is_some()
            || client_message.is_empty()
            || server_message.is_empty()
        {
            return Err("socket requires a GET handshake with no body or finite responses");
        }
        route.options.mcp = false;
        route.options.openapi = false;
        let mut route =
            Endpoint::<I, NoBody>::from_contract(route.with_response(204, None))?.into_contract();
        route.responses.clear();
        let mut socket = SocketContract::new(&route.path)
            .with_client_message(
                client_message,
                SocketSchemaEntry::json(crate::contract::schema_value_for::<In>()),
            )
            .with_server_message(
                server_message,
                SocketSchemaEntry::json(crate::contract::schema_value_for::<Out>()),
            );
        socket.handshake = Some(
            serde_json::from_str(&route.to_json())
                .map_err(|_| "invalid socket handshake metadata")?,
        );
        Ok(Self {
            route,
            socket,
            marker: PhantomData,
        })
    }
    #[must_use]
    pub fn contract(&self) -> SocketContract {
        self.socket.clone()
    }
    #[must_use]
    pub fn into_parts(self) -> (RouteContract, SocketContract) {
        (self.route, self.socket)
    }
}
