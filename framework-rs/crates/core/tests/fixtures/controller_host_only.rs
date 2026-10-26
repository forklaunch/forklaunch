use absent_server_dependency::ServerState;
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
pub fn handler(_: ServerState) {
    absent_server_dependency::start_server();
}
