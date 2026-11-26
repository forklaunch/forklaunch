#![cfg(all(feature = "typed-schema", feature = "schema-validation"))]
use forklaunch_core::contract::{
    ContractSchema, schema,
    schema::{Optional, Patch},
    schema_validation::compile_checked,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

type Byte = u8;
type Short = u16;
type Count = u32;
type SignedByte = i8;
type SignedShort = i16;
type SignedCount = i32;

#[schema(unknown_fields = "deny")]
struct Wrapped {
    #[field(optional, min_length = 1, max_length = 4)]
    optional_text: Optional<String>,
    #[field(min = 0)]
    safe_cursor: Option<i64>,
    #[field(max = 100)]
    safe_unsigned: Option<u64>,
    byte: Option<Byte>,
    short: Option<Short>,
    count: Option<Count>,
    signed_byte: Option<SignedByte>,
    signed_short: Option<SignedShort>,
    signed_count: Option<SignedCount>,
    groups: Vec<BTreeMap<String, Option<Byte>>>,
    boxed: Box<SignedCount>,
    #[field(patch)]
    patch: Patch<Option<Short>>,
}

#[test]
fn aliases_optional_collections_and_patch_keep_native_integer_bounds() {
    let schema = forklaunch_core::contract::schema_value_for::<Wrapped>();
    let declaration: Value =
        serde_json::from_str(Wrapped::schema_ref().json_schema.as_ref().unwrap()).unwrap();
    assert_eq!(declaration, schema);
    let validate = compile_checked(&schema).unwrap();
    let baseline = json!({"groups":[],"boxed":0});
    let mut cases = vec![baseline.clone()];
    for (field, minimum, maximum) in [
        ("byte", 0_i64, 255_i64),
        ("short", 0, 65_535),
        ("count", 0, 4_294_967_295),
        ("signed_byte", -128, 127),
        ("signed_short", -32_768, 32_767),
        ("signed_count", -2_147_483_648, 2_147_483_647),
        ("patch", 0, 65_535),
        ("boxed", -2_147_483_648, 2_147_483_647),
    ] {
        for value in [
            json!(minimum - 1),
            json!(minimum),
            json!(maximum),
            json!(maximum + 1),
            Value::Null,
            json!(1.5),
        ] {
            let mut input = baseline.clone();
            input[field] = value;
            cases.push(input);
        }
    }
    for value in [
        json!(-1),
        json!(0),
        json!(255),
        json!(256),
        Value::Null,
        json!(true),
    ] {
        let mut input = baseline.clone();
        input["groups"] = json!([{"entry":value}]);
        cases.push(input);
    }
    for value in [
        json!(null),
        json!(""),
        json!("hello"),
        json!("ok"),
        json!("🍎🍎🍎🍎"),
        json!(true),
        json!(42),
    ] {
        let mut input = baseline.clone();
        input["optional_text"] = value;
        cases.push(input);
    }
    for value in [
        json!(null),
        json!(0),
        json!(-1),
        json!(9_007_199_254_740_991_i64),
        json!(9_007_199_254_740_992_i64),
        json!(i64::MAX),
        json!(1.5),
    ] {
        let mut input = baseline.clone();
        input["safe_cursor"] = value;
        cases.push(input);
    }
    for value in [
        json!(null),
        json!(-1),
        json!(0),
        json!(100),
        json!(101),
        json!(u64::MAX),
    ] {
        let mut input = baseline.clone();
        input["safe_unsigned"] = value;
        cases.push(input);
    }
    let cases = cases
        .into_iter()
        .map(|input| {
            let accepted = serde_json::from_value::<Wrapped>(input.clone()).is_ok();
            assert_eq!(
                validate(&input).is_ok(),
                accepted,
                "wire/schema mismatch: {input}"
            );
            json!({"input":input,"accepted":accepted})
        })
        .collect::<Vec<_>>();
    if let Ok(path) = std::env::var("FORKLAUNCH_INTEGER_FIXTURE") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&json!({"schema":schema,"cases":cases})).unwrap(),
        )
        .unwrap();
    }
}

#[cfg(feature = "build")]
#[test]
fn integer_bound_assets_reach_both_service_clients() {
    use forklaunch_core::{
        codegen::{SdkGenerationPlan, SdkTarget},
        contract::{BodyContract, ContractRegistry, RouteContract, ServiceContract},
    };
    let mut registry = ContractRegistry::for_service(ServiceContract::new("integers", "1"));
    registry.register(
        RouteContract::get("GetWrapped", "Read wrapped integers", "/wrapped")
            .with_response(200, Some(BodyContract::json_type::<Wrapped>())),
    );
    if let Ok(path) = std::env::var("FORKLAUNCH_INTEGER_SDK_DIR") {
        let path = std::path::Path::new(&path);
        std::fs::create_dir_all(path).unwrap();
        let ts = forklaunch_core::typescript::generate_typescript_http_sdk_source(
            &SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry),
            &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("integers"),
        )
        .unwrap();
        let rust = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
            &SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry),
            &forklaunch_core::rust_sdk::RustSdkOptions::new("integer-proof"),
        )
        .unwrap();
        std::fs::write(path.join("client.ts"), ts).unwrap();
        std::fs::write(path.join("client.rs"), rust).unwrap();
    }
}
