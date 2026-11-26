use crate::ServerOnly;

#[forklaunch_core::contract::api]
pub mod contract {
    use forklaunch_core::contract::{
        HttpMethod,
        endpoint::{Endpoint, NoBody, TextBody},
    };
    pub fn hello() -> Endpoint<NoBody, TextBody> {
        Endpoint::new("hello", "hello", HttpMethod::Get, "/hello", 200).unwrap()
    }
}

pub fn handler(state: ServerOnly) -> String {
    state.0
}
