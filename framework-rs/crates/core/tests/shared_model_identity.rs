#![cfg(all(feature = "build", feature = "typed-schema"))]
use forklaunch_core::{
    codegen::{SdkGenerationPlan, SdkTarget},
    contract::{
        self, ContractRegistry, HttpMethod, ServiceContract,
        endpoint::{Endpoint, JsonBody, NoBody, SseBody},
    },
};

#[contract::schema(unknown_fields = "deny")]
struct Person {
    #[field(min_length = 1)]
    name: String,
}
#[contract::schema(unknown_fields = "deny")]
struct Label {
    name: String,
}

#[contract::schema(unknown_fields = "deny")]
struct Tree {
    value: String,
    child: Option<Box<Tree>>,
}

#[contract::schema(unknown_fields = "deny")]
struct Response {
    ok: bool,
}

#[contract::schema(unknown_fields = "deny")]
struct Date {
    at: chrono::DateTime<chrono::Utc>,
}
#[contract::schema(unknown_fields = "deny")]
struct Headers {
    value: String,
}
#[contract::schema(unknown_fields = "deny")]
struct Ajv {
    value: String,
}

#[contract::schema(unknown_fields = "deny")]
struct PersonEnvelope {
    person: Person,
    people: Vec<Person>,
    optional: Option<Person>,
}
#[contract::schema(unknown_fields = "deny")]
struct PersonPage {
    selected: Person,
    entries: Vec<Person>,
}

#[contract::schema(unknown_fields = "deny")]
struct Settings {
    #[field(default)]
    count: u32,
}
#[contract::schema(unknown_fields = "deny")]
struct SettingsEnvelope {
    settings: Settings,
}

#[allow(clippy::too_many_lines)] // One registry fixture.
fn plan() -> SdkGenerationPlan {
    let mut registry = ContractRegistry::for_service(ServiceContract::new("identity", "1"));
    for (operation, path) in [("create", "/people"), ("read", "/people/read")] {
        registry.register(
            Endpoint::<NoBody, JsonBody<Person>>::new(
                operation,
                operation,
                HttpMethod::Get,
                path,
                200,
            )
            .unwrap()
            .contract(),
        );
    }
    registry.register(
        Endpoint::<NoBody, JsonBody<Label>>::new("label", "Label", HttpMethod::Get, "/labels", 200)
            .unwrap()
            .contract(),
    );
    for (operation, path) in [("create_tree", "/trees"), ("read_tree", "/trees/read")] {
        registry.register(
            Endpoint::<NoBody, JsonBody<Tree>>::new(
                operation,
                operation,
                HttpMethod::Get,
                path,
                200,
            )
            .unwrap()
            .contract(),
        );
    }
    registry.register(
        Endpoint::<NoBody, JsonBody<Response>>::new(
            "status",
            "Status",
            HttpMethod::Get,
            "/status",
            200,
        )
        .unwrap()
        .contract(),
    );
    registry.register(
        Endpoint::<NoBody, JsonBody<Date>>::new("date", "Date", HttpMethod::Get, "/dates", 200)
            .unwrap()
            .contract(),
    );
    registry.register(
        Endpoint::<NoBody, JsonBody<Headers>>::new(
            "headers",
            "Headers",
            HttpMethod::Get,
            "/headers",
            200,
        )
        .unwrap()
        .contract(),
    );
    registry.register(
        Endpoint::<NoBody, JsonBody<Ajv>>::new("ajv", "Ajv", HttpMethod::Get, "/ajv", 200)
            .unwrap()
            .contract(),
    );
    registry.register(
        Endpoint::<NoBody, SseBody<Person>>::new(
            "watch_people",
            "Watch",
            HttpMethod::Get,
            "/people/events",
            200,
        )
        .unwrap()
        .contract(),
    );
    registry.register(
        Endpoint::<NoBody, SseBody<Tree>>::new(
            "watch_tree",
            "Watch Tree",
            HttpMethod::Get,
            "/trees/events",
            200,
        )
        .unwrap()
        .contract(),
    );
    registry.register(
        Endpoint::<NoBody, JsonBody<PersonEnvelope>>::new(
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
        Endpoint::<NoBody, JsonBody<PersonPage>>::new(
            "page",
            "Page",
            HttpMethod::Get,
            "/page",
            200,
        )
        .unwrap()
        .contract(),
    );
    registry.register(
        Endpoint::<JsonBody<SettingsEnvelope>, JsonBody<SettingsEnvelope>>::new(
            "settings",
            "Settings",
            HttpMethod::Post,
            "/settings",
            200,
        )
        .unwrap()
        .contract(),
    );
    SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry)
}

#[test]
fn named_models_reuse_types_and_keep_operation_aliases() {
    let plan = plan();
    let rust = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &plan,
        &forklaunch_core::rust_sdk::RustSdkOptions::new("identity"),
    )
    .unwrap();
    let ts = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &plan,
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("identity"),
    )
    .unwrap();
    assert_eq!(rust.matches("pub struct Person {").count(), 1);
    assert!(rust.contains("pub type CreateResponse = Person;"));
    assert!(rust.contains("pub type ReadResponse = Person;"));
    assert!(rust.contains("pub struct Label {"));
    assert!(rust.contains("pub struct PersonEnvelope {"));
    assert_eq!(rust.matches("pub struct Tree {").count(), 1);
    assert!(ts.contains("export type Person ="));
    assert!(ts.contains("export type Label ="));
    if let Ok(directory) = std::env::var("FORKLAUNCH_MODEL_IDENTITY_DIR") {
        std::fs::write(format!("{directory}/client.rs"), rust).unwrap();
        std::fs::write(format!("{directory}/client.ts"), ts).unwrap();
    }
}

#[test]
fn same_identity_with_different_constraints_is_rejected_in_both_languages() {
    let mut plan = plan();
    let response = &mut plan.operations[1].responses[0];
    let mut schema: serde_json::Value =
        serde_json::from_str(response.body_json_schema.as_ref().unwrap()).unwrap();
    schema["properties"]["name"]["minLength"] = serde_json::json!(3);
    response.body_json_schema = Some(schema.to_string());
    let rust = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &plan,
        &forklaunch_core::rust_sdk::RustSdkOptions::new("identity"),
    );
    let ts = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &plan,
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("identity"),
    );
    assert!(
        rust.unwrap_err()
            .to_string()
            .contains("Conflicting schemas")
    );
    assert!(ts.unwrap_err().to_string().contains("Conflicting schemas"));
}

#[test]
fn conflicting_public_names_do_not_merge_distinct_identities() {
    let mut plan = plan();
    let response = &mut plan.operations[1].responses[0];
    let mut schema: serde_json::Value =
        serde_json::from_str(response.body_json_schema.as_ref().unwrap()).unwrap();
    schema["x-forklaunch-model"]["id"] = serde_json::json!("other_module::Person");
    response.body_json_schema = Some(schema.to_string());
    let error = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &plan,
        &forklaunch_core::rust_sdk::RustSdkOptions::new("identity"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("Ambiguous generated model name"));
}

#[test]
fn renamed_runtime_name_collision_is_rejected() {
    let mut plan = plan();
    for operation in &mut plan.operations[..2] {
        let response = &mut operation.responses[0];
        let mut schema: serde_json::Value =
            serde_json::from_str(response.body_json_schema.as_ref().unwrap()).unwrap();
        schema["x-forklaunch-model"]["name"] = serde_json::json!("ResponseModel");
        response.body_json_schema = Some(schema.to_string());
    }
    let error = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &plan,
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("identity"),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Ambiguous generated model name ResponseModel")
    );
}

#[test]
fn nested_identity_conflicts_keep_constraints_and_literals_intact() {
    let mut plan = plan();
    let operation = plan
        .operations
        .iter_mut()
        .find(|operation| operation.name == "page");
    let operation = operation.expect("page operation");
    let response = &mut operation.responses[0];
    let mut schema: serde_json::Value =
        serde_json::from_str(response.body_json_schema.as_ref().unwrap()).unwrap();
    schema["definitions"]["Person"]["properties"]["name"]["minLength"] = serde_json::json!(9);
    response.body_json_schema = Some(schema.to_string());
    assert!(
        forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
            &plan,
            &forklaunch_core::rust_sdk::RustSdkOptions::new("identity")
        )
        .unwrap_err()
        .to_string()
        .contains("Conflicting schemas")
    );
}

#[test]
fn managed_failure_and_nested_models_retain_dialect_inside_error_union() {
    #[allow(dead_code)] // Descriptor construction intentionally exercises every declared variant.
    #[derive(contract::ForklaunchResponses)]
    enum Reply {
        #[response(status = 200)]
        Found(Person),
        #[response(status = 400)]
        Invalid(PersonEnvelope),
    }
    let mut registry = ContractRegistry::for_service(ServiceContract::new("errors", "1"));
    registry.register(
        Endpoint::<NoBody, contract::endpoint::Responses<Reply>>::responses(
            "custom",
            "Custom",
            HttpMethod::Get,
            "/custom",
        )
        .unwrap()
        .contract(),
    );
    registry.register(
        Endpoint::<NoBody, JsonBody<Person>>::new(
            "ordinary",
            "Ordinary",
            HttpMethod::Get,
            "/ordinary",
            200,
        )
        .unwrap()
        .contract(),
    );
    let plan = SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry);
    let rust = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &plan,
        &forklaunch_core::rust_sdk::RustSdkOptions::new("errors"),
    )
    .unwrap();
    let ts = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &plan,
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("errors"),
    )
    .unwrap();
    let artifact = registry.into_artifact().to_json();
    let imported = SdkGenerationPlan::from_artifact_json(SdkTarget::Rust, &artifact).unwrap();
    assert_eq!(
        rust,
        forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
            &imported,
            &forklaunch_core::rust_sdk::RustSdkOptions::new("errors"),
        )
        .unwrap()
    );
    assert_eq!(
        ts,
        forklaunch_core::typescript::generate_typescript_http_sdk_source(
            &imported,
            &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("errors"),
        )
        .unwrap()
    );
}
