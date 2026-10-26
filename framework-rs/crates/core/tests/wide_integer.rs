#![cfg(all(feature = "build", feature = "typed-schema"))]
use forklaunch_core::{
    codegen::{SdkGenerationPlan, SdkTarget},
    contract::{BodyContract, ContractRegistry, RouteContract, ServiceContract, schema},
};
use serde_json::json;
#[schema]
struct Unsafe {
    signed: i64,
    unsigned: u64,
}
#[schema]
struct Safe {
    #[field(min = 0)]
    count: u64,
}
#[test]
fn unsafe_native_integer_export_fails_in_both_clients() {
    let mut registry = ContractRegistry::for_service(ServiceContract::new("integers", "1"));
    registry.register(
        RouteContract::get("get", "Get", "/get")
            .with_response(200, Some(BodyContract::json_type::<Unsafe>())),
    );
    let ts = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry),
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("integers"),
    );
    let rust = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry),
        &forklaunch_core::rust_sdk::RustSdkOptions::new("integers"),
    );
    assert!(ts.unwrap_err().to_string().contains("wide native integer"));
    assert!(
        rust.unwrap_err()
            .to_string()
            .contains("wide native integer")
    );
}
#[test]
fn unsigned_codec_uses_checked_widening_before_bounds() {
    let schema = forklaunch_core::contract::schema_value_for::<Safe>();
    let validate = forklaunch_core::contract::schema_validation::compile_checked(&schema).unwrap();
    for (count, valid) in [
        (json!(0), true),
        (json!(-1), false),
        (json!(9_007_199_254_740_991_u64), true),
        (json!(9_007_199_254_740_992_u64), false),
        (json!(u64::MAX), false),
    ] {
        let input = json!({"count":count});
        assert_eq!(validate(&input).is_ok(), valid);
        assert_eq!(serde_json::from_value::<Safe>(input).is_ok(), valid);
    }
    assert!(serde_json::to_value(Safe { count: u64::MAX }).is_err());
}
