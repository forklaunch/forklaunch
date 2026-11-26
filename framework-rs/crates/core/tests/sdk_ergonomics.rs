#![cfg(all(feature = "build", feature = "typed-schema"))]
use forklaunch_core::{
    codegen::{SdkGenerationPlan, SdkTarget},
    contract::{
        ContractRegistry, HttpMethod,
        endpoint::{Endpoint, JsonBody, NoBody},
    },
    rust_sdk::{RustSdkOptions, generate_rust_http_sdk_files},
};

#[forklaunch_core::contract::schema(rename_all = "camelCase")]
pub struct Health {
    #[field(max = 9007199254740991)]
    pub record_count: usize,
}

#[test]
fn no_input_client_preserves_schema_and_hides_decoder_machinery() {
    let mut registry = ContractRegistry::new();
    registry.register(
        Endpoint::<NoBody, JsonBody<Health>>::new(
            "Get Health",
            "Health",
            HttpMethod::Get,
            "/health",
            200,
        )
        .unwrap()
        .contract(),
    );
    registry.register(
        Endpoint::<JsonBody<Health>, JsonBody<Health>>::new(
            "Echo",
            "Echo",
            HttpMethod::Post,
            "/echo",
            200,
        )
        .unwrap()
        .contract(),
    );
    let plan = SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry);
    let files = generate_rust_http_sdk_files(
        &plan,
        &RustSdkOptions::new("sdk-ergonomics").with_client_struct("Client"),
    )
    .unwrap();
    let models = &files["models.rs"];
    assert!(models.contains("pub record_count: u64"));
    assert!(!models.contains("GetHealthStatus400Response"));
    assert!(models.contains("Status400(Box<EndpointFailure>)"));
    let request = models
        .split("pub struct GetHealthRequest {")
        .nth(1)
        .unwrap()
        .split('}')
        .next()
        .unwrap();
    assert!(request.contains("pub headers:"));
    assert!(
        !request.contains("body:") && !request.contains("params:") && !request.contains("query:")
    );
    let client = &files["client.rs"];
    assert!(client.contains("pub async fn get_health(&self)"));
    assert!(client.contains("pub async fn get_health_with_options("));
    assert!(client.contains("runtime::decode_json_response"));
    assert!(!client.contains("serde_json::from_value"));
    assert!(!client.contains("$schema"));
    assert!(files["schemas.rs"].contains("GET_HEALTH_STATUS200_BODY"));
    assert!(files["lib.rs"].contains("mod schemas;"));
    if let Some(root) = std::env::var_os("FORKLAUNCH_ERGONOMIC_SDK_FIXTURE") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(root.join("tests")).unwrap();
        for (name, source) in files {
            std::fs::write(root.join(name), source).unwrap();
        }
        std::fs::write(
            root.join("tests/runtime.rs"),
            include_str!("../compiler-tests/package_sdk/ergonomics.rs"),
        )
        .unwrap();
    }
}

#[test]
fn generated_convenience_method_cannot_shadow_a_declared_operation() {
    let mut registry = ContractRegistry::new();
    for name in ["Get Health", "Get Health With Options"] {
        registry.register(
            Endpoint::<NoBody, JsonBody<Health>>::new(
                name,
                "Health",
                HttpMethod::Get,
                &format!("/{name}"),
                200,
            )
            .unwrap()
            .contract(),
        );
    }
    assert!(
        generate_rust_http_sdk_files(
            &SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry),
            &RustSdkOptions::new("test")
        )
        .is_err()
    );
}
