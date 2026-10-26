//! Host-compiled stream message and handshake contracts.
use super::routes::{EVENTS, PRESENCE, route};
use crate::schemas::{NoteEvent, PresenceEvent, PresenceInput, WatchHeaders};
use framework_core::contract::endpoint::{
    Endpoint, EndpointBody, Fields, NoBody, NoFields, SseBody,
};
use framework_core::contract::{AccessLevel, BodyContract, ContentType, HttpMethod};

pub type WatchRequest = Fields<NoBody, NoFields, WatchHeaders>;
pub type WatchInput = <WatchRequest as EndpointBody>::Value;
#[framework_core::contract::api]
pub mod streams {
    use super::*;
    #[access(Authenticated)]
    pub fn watch_notes() -> Endpoint<WatchRequest, SseBody<NoteEvent>> {
        let mut route = route("Watch Notes", HttpMethod::Get, EVENTS).with_response(
            200,
            Some(BodyContract::typed::<NoteEvent>(ContentType::EventStream)),
        );
        route.request.headers = WatchRequest::header_fields().expect("valid watch headers");
        Endpoint::from_contract(route)
            .expect("typed note event contract")
            .with_response_header("cache-control", "private, no-store")
            .expect("cache policy")
            .with_response_header("x-accel-buffering", "no")
            .expect("stream buffering policy")
    }
}
pub use streams::watch_notes;
#[ws::socket_api]
pub mod sockets {
    use super::*;
    pub fn presence() -> ws::TypedSocket<NoBody, PresenceInput, PresenceEvent> {
        let mut route =
            route("Presence", HttpMethod::Get, PRESENCE).with_access(AccessLevel::Authenticated);
        route.responses.clear();
        ws::TypedSocket::new(route, "presence", "presence").expect("typed presence handshake")
    }
}
pub use sockets::presence as presence_endpoint;
pub fn presence_contract() -> ws::SocketContract {
    presence_endpoint().contract()
}
