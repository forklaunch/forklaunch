#![cfg(feature = "typed-schema")]
use forklaunch_core::contract::controller_contract;
struct ServerOnly(String);
#[path = "fixtures/controller_inline.rs"]
mod runtime;
controller_contract!(host, "tests/fixtures/controller_inline.rs");
controller_contract!(server_free, "tests/fixtures/controller_host_only.rs");
#[test]
fn inline_runtime_and_host_graphs_match_without_compiling_handler_dependencies() {
    assert_eq!(
        runtime::contract::registry().into_artifact(),
        host::contract::registry().into_artifact()
    );
    assert_eq!(
        host::contract::registry().into_artifact(),
        server_free::contract::registry().into_artifact()
    );
    assert_eq!(runtime::handler(ServerOnly("runtime".into())), "runtime");
    let declaration = host::contract::binding_manifest().unwrap().remove(0);
    assert_eq!(declaration.source, "tests/fixtures/controller_inline.rs");
    assert_eq!(declaration.group, "contract");
}
