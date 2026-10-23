#![cfg(all(feature = "build", feature = "typed-schema"))]
use forklaunch_core::{
    codegen::{SdkGenerationPlan, SdkTarget},
    contract::{
        self, ContractRegistry, HttpMethod, ServiceContract,
        endpoint::{Endpoint, FormBody, JsonBody},
    },
};
#[contract::schema]
struct Profile {
    name: String,
    ages: Vec<u16>,
    enabled: bool,
}
#[contract::schema]
struct Form {
    profile: Profile,
    labels: Vec<String>,
}
#[test]
fn nested_forms_emit_typed_clients() {
    let endpoint = Endpoint::<FormBody<Form>, JsonBody<Form>>::new(
        "apply",
        "Apply",
        HttpMethod::Post,
        "/forms/apply",
        200,
    )
    .unwrap();
    let mut registry = ContractRegistry::for_service(ServiceContract::new("forms", "1"));
    registry.register(endpoint.contract());
    let ts = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry),
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("forms"),
    )
    .unwrap();
    let rust = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry),
        &forklaunch_core::rust_sdk::RustSdkOptions::new("forms"),
    )
    .unwrap();
    assert!(rust.contains("runtime::encode_urlencoded(&body)"));
    assert!(rust.contains("ApplyRequestBody"));
    if let Ok(directory) = std::env::var("FORKLAUNCH_FORM_SDK_DIR") {
        let directory = std::path::Path::new(&directory);
        std::fs::write(directory.join("client.ts"), ts).unwrap();
        std::fs::write(directory.join("client.rs"), rust).unwrap();
    }
}
