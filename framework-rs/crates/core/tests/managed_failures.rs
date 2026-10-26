#![cfg(all(feature = "build", feature = "typed-schema"))]
use forklaunch_core::{
    codegen::{SdkGenerationPlan, SdkTarget},
    contract::{
        self, ContractRegistry, HttpMethod, ServiceContract,
        endpoint::{Endpoint, NoBody},
    },
};

#[contract::schema]
struct Headers {
    #[serde(rename = "x-request-id")]
    request_id: String,
}

#[test]
fn managed_timeout_has_checked_sdk_schema_without_application_header_requirements() {
    let endpoint = Endpoint::<NoBody, NoBody>::new(
        "pending",
        "Pending",
        HttpMethod::Get,
        "/requests/pending",
        200,
    )
    .unwrap()
    .with_typed_response_headers::<Headers>()
    .unwrap();
    let timeout = endpoint
        .contract()
        .responses
        .into_iter()
        .find(|response| response.status_code == 408)
        .unwrap();
    assert!(timeout.headers.iter().all(|header| !header.required));
    let declared = Endpoint::<NoBody, contract::endpoint::JsonBody<String>>::new(
        "domain_timeout",
        "Domain timeout",
        HttpMethod::Get,
        "/domain-timeout",
        408,
    )
    .unwrap()
    .with_typed_response_headers::<Headers>()
    .unwrap()
    .contract();
    let declared_timeout = declared
        .responses
        .iter()
        .find(|response| response.status_code == 408)
        .unwrap();
    assert_eq!(declared_timeout.headers.len(), 1);
    assert!(!declared_timeout.headers[0].required);

    let mut registry = ContractRegistry::for_service(ServiceContract::new("managed", "1"));
    registry.register(endpoint.contract());
    let ts = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry),
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("managed"),
    )
    .unwrap();
    let rust = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry),
        &forklaunch_core::rust_sdk::RustSdkOptions::new("managed"),
    )
    .unwrap();
    assert!(ts.contains("408:application/json"));
    assert!(rust.contains("Status408"));
    if let Ok(directory) = std::env::var("FORKLAUNCH_MANAGED_SDK_DIR") {
        let directory = std::path::Path::new(&directory);
        std::fs::write(directory.join("client.ts"), ts).unwrap();
        std::fs::write(directory.join("client.rs"), rust).unwrap();
    }
}
