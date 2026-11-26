#![cfg(all(feature = "build", feature = "typed-schema"))]
use forklaunch_core::{
    codegen::{SdkGenerationPlan, SdkTarget},
    contract::{
        self, ContractRegistry, HttpMethod, ServiceContract,
        endpoint::{Endpoint, JsonBody, NoBody},
    },
};
#[contract::schema]
struct Person {
    name: String,
}
#[contract::schema]
struct Page<T> {
    items: Vec<T>,
}
#[contract::schema]
struct Pair<A, B> {
    first: A,
    second: B,
}
#[contract::schema]
struct Revision<const N: usize> {
    value: String,
}
#[contract::schema]
struct Envelope {
    page: Page<Person>,
    strings: Page<String>,
    pair: Pair<Person, String>,
    first: Revision<1>,
    second: Revision<2>,
}
#[test]
fn instantiated_generic_models_have_nested_identity_and_reusable_types() {
    let schema = contract::schema_value_for::<Envelope>();
    for name in [
        "Page_for_Person",
        "Page_for_String",
        "Pair_for_Person_and_String",
        "Revision_for_1",
        "Revision_for_2",
    ] {
        assert_eq!(
            schema["definitions"][name]["x-forklaunch-model"]["name"],
            name
        );
    }
    assert_ne!(
        schema["definitions"]["Revision_for_1"]["x-forklaunch-model"]["id"],
        schema["definitions"]["Revision_for_2"]["x-forklaunch-model"]["id"]
    );
    assert_eq!(
        contract::schema_value_for::<Page<Person>>()["x-forklaunch-model"],
        schema["definitions"]["Page_for_Person"]["x-forklaunch-model"]
    );
    let mut registry = ContractRegistry::for_service(ServiceContract::new("generic", "1"));
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
        Endpoint::<NoBody, JsonBody<Page<Person>>>::new(
            "people",
            "People",
            HttpMethod::Get,
            "/people",
            200,
        )
        .unwrap()
        .contract(),
    );
    let plan = SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry);
    let rust = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &plan,
        &forklaunch_core::rust_sdk::RustSdkOptions::new("generic"),
    )
    .unwrap();
    let ts = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &plan,
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("generic"),
    )
    .unwrap();
    assert_eq!(rust.matches("pub struct PageForPerson {").count(), 1);
    assert!(rust.contains("pub page: PageForPerson,"));
    assert!(rust.contains("pub type PeopleResponse = PageForPerson;"));
    assert!(ts.contains("export type PageForPerson ="));
    if let Ok(directory) = std::env::var("FORKLAUNCH_GENERIC_MODEL_DIR") {
        std::fs::write(format!("{directory}/client.rs"), rust).unwrap();
        std::fs::write(format!("{directory}/client.ts"), ts).unwrap();
    }
}
