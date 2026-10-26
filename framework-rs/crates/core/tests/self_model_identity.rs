#![cfg(all(feature = "build", feature = "typed-schema"))]
use forklaunch_core::{
    codegen::{SdkGenerationPlan, SdkTarget},
    contract::{
        self, ContractRegistry, HttpMethod, ServiceContract,
        endpoint::{Endpoint, JsonBody, NoBody},
    },
};
#[contract::schema]
struct Node {
    value: String,
    child: Option<Box<Self>>,
}
#[contract::schema]
struct GenericNode<T> {
    value: T,
    children: Vec<Self>,
}
#[contract::schema]
struct Envelope {
    root: Node,
    generic: GenericNode<String>,
}
#[test]
fn recursive_self_uses_public_dto_identity_at_root_and_nested_positions() {
    let schema = contract::schema_value_for::<Envelope>();
    for name in ["Node", "GenericNode_for_String"] {
        assert_eq!(
            schema["definitions"][name]["x-forklaunch-model"]["name"],
            name
        );
    }
    let mut registry = ContractRegistry::for_service(ServiceContract::new("recursive", "1"));
    registry.register(
        Endpoint::<NoBody, JsonBody<Envelope>>::new(
            "envelope",
            "Envelope",
            HttpMethod::Get,
            "/envelope",
            200,
        )
        .unwrap()
        .contract(),
    );
    registry.register(
        Endpoint::<NoBody, JsonBody<Node>>::new("node", "Node", HttpMethod::Get, "/node", 200)
            .unwrap()
            .contract(),
    );
    registry.register(
        Endpoint::<NoBody, JsonBody<GenericNode<String>>>::new(
            "generic",
            "Generic",
            HttpMethod::Get,
            "/generic",
            200,
        )
        .unwrap()
        .contract(),
    );
    let plan = SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry);
    let rust = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &plan,
        &forklaunch_core::rust_sdk::RustSdkOptions::new("recursive"),
    )
    .unwrap();
    let ts = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &plan,
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("recursive"),
    )
    .unwrap();
    assert_eq!(rust.matches("pub struct Node {").count(), 1);
    assert_eq!(rust.matches("pub struct GenericNodeForString {").count(), 1);
    assert!(!rust.contains("ForklaunchSchema"));
    assert!(!ts.contains("ForklaunchSchema"));
    if let Ok(directory) = std::env::var("FORKLAUNCH_SELF_MODEL_DIR") {
        std::fs::write(format!("{directory}/client.rs"), rust).unwrap();
        std::fs::write(format!("{directory}/client.ts"), ts).unwrap();
    }
}
