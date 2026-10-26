#![cfg(all(feature = "typescript", feature = "rust-sdk", feature = "typed-schema"))]
use forklaunch_core::{
    codegen::{SdkGenerationPlan, SdkTarget},
    contract::{
        BodyContract, BodyKind, ContractRegistry, FieldContract, HttpMethod, RouteContract,
        SchemaRef,
        endpoint::{Endpoint, JsonBody, NoBody},
    },
};

#[forklaunch_core::contract::schema]
pub struct Item {
    #[field(min_length = 3)]
    pub name: String,
}

#[test]
fn package_files_preserve_contracts_and_share_one_runtime() {
    let endpoint =
        Endpoint::<NoBody, JsonBody<Item>>::new("Fetch", "Fetch", HttpMethod::Get, "/items", 200)
            .unwrap();
    let mut registry = ContractRegistry::new();
    registry.register(endpoint.contract());
    let plan = SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry);
    let ts = forklaunch_core::typescript::generate_typescript_http_sdk_files(
        &plan,
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("items"),
    )
    .unwrap();
    let second = forklaunch_core::typescript::generate_typescript_http_sdk_files(
        &plan,
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("other"),
    )
    .unwrap();
    assert_eq!(ts["runtime.ts"], second["runtime.ts"]);
    assert!(
        ts["index.ts"]
            .lines()
            .all(|line| line.starts_with("export "))
    );
    assert!(!ts["client.ts"].contains("function request<"));
    assert!(ts["models.ts"].contains("export type Item ="));
    let rust = forklaunch_core::rust_sdk::generate_rust_http_sdk_files(
        &SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry),
        &forklaunch_core::rust_sdk::RustSdkOptions::new("items-sdk"),
    )
    .unwrap();
    assert!(!rust["lib.rs"].contains("pub struct"));
    assert!(rust["models.rs"].contains("pub struct Item"));
    assert!(!rust["client.rs"].contains("pub struct Item"));
    if let Some(root) = std::env::var_os("FORKLAUNCH_PACKAGE_SDK_FIXTURE") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(root.join("typescript/items")).unwrap();
        std::fs::create_dir_all(root.join("rust/tests")).unwrap();
        std::fs::write(
            root.join("typescript/consumer.ts"),
            include_str!("../compiler-tests/package_sdk/consumer.ts"),
        )
        .unwrap();
        #[cfg(feature = "build")]
        std::fs::write(
            root.join("rust/tests/runtime.rs"),
            include_str!("../compiler-tests/package_sdk/runtime.rs"),
        )
        .unwrap();
        for (name, source) in ts {
            let path = if name == "runtime.ts" {
                root.join("typescript").join(name)
            } else {
                root.join("typescript/items").join(name)
            };
            std::fs::write(path, source).unwrap();
        }
        #[cfg(feature = "build")]
        forklaunch_core::build::rust_http_module(
            &registry,
            "items-sdk",
            "ForkLaunchClient",
            root.join("rust/fixture/generated.rs"),
        )
        .unwrap();
        for (name, source) in rust {
            std::fs::write(root.join("rust").join(name), source).unwrap();
        }
    }
}

#[test]
fn empty_package_has_no_unused_model_imports() {
    let files = forklaunch_core::rust_sdk::generate_rust_http_sdk_files(
        &SdkGenerationPlan::from_registry(SdkTarget::Rust, &ContractRegistry::new()),
        &forklaunch_core::rust_sdk::RustSdkOptions::new("empty-sdk"),
    )
    .unwrap();
    assert!(!files["models.rs"].contains("use std::fmt;"));
    assert!(!files["models.rs"].contains("use std::collections::BTreeMap;"));
    if let Some(root) = std::env::var_os("FORKLAUNCH_PACKAGE_SDK_FIXTURE") {
        let root = std::path::PathBuf::from(root).join("rust/empty");
        std::fs::create_dir_all(&root).unwrap();
        for (name, source) in files {
            std::fs::write(root.join(name), source).unwrap();
        }
    }
}

#[forklaunch_core::contract::schema]
pub struct TimedItem {
    pub at: forklaunch_core::contract::datetime::DateTime<forklaunch_core::contract::datetime::Utc>,
}

#[test]
fn portable_service_is_type_only_and_runtime_is_shared() {
    let endpoint =
        Endpoint::<NoBody, JsonBody<Item>>::new("Fetch", "Fetch", HttpMethod::Get, "/items", 200)
            .unwrap();
    let mut registry = ContractRegistry::new();
    registry.register(endpoint.contract());
    registry.register(
        Endpoint::<JsonBody<TimedItem>, JsonBody<TimedItem>>::new(
            "Time",
            "Time",
            HttpMethod::Post,
            "/items/time",
            200,
        )
        .unwrap()
        .contract(),
    );
    registry.register(RouteContract::get("Headed", "Headed", "/items/headed")
        .with_response(200, Some(BodyContract {schema:SchemaRef::named("Item").with_json_schema(
            r#"{"type":"object","required":["name"],"properties":{"name":{"type":"string","minLength":3}}}"#), ..BodyContract::json("Item")}))
        .with_response_header(200, FieldContract { name: "x-revision".into(), required: true, schema: SchemaRef::named("Revision").with_json_schema(r#"{"type":"integer","minimum":1,"maximum":4294967295}"#) }));
    registry.register(RouteContract::get("Watch", "Watch", "/items/watch")
        .with_response(200, Some(BodyContract {
            kind:BodyKind::ServerSentEvent, content_type:Some("text/event-stream".into()),
            schema:SchemaRef::named("ItemEvent").with_json_schema(r#"{"x-forklaunch-sse-framing":"payload","type":"object","required":["name"],"properties":{"name":{"type":"string","minLength":3}}}"#)
        })));
    let item_body = endpoint.contract().responses[0].body.clone();
    registry.register(
        RouteContract::get("Represent", "Represent", "/items/represent")
            .with_response(200, item_body.clone())
            .with_response(
                200,
                Some(BodyContract {
                    kind: BodyKind::Text,
                    content_type: Some("text/plain".into()),
                    schema: SchemaRef::named("Text").with_json_schema(r#"{"type":"string"}"#),
                }),
            )
            .with_response(201, item_body),
    );
    let plan = SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry);
    let options = forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("items");
    let declarations =
        forklaunch_core::typescript::generate_typescript_service_declarations(&plan, &options)
            .unwrap();
    assert!(declarations.contains("export type ItemsSdkClient"));
    assert!(declarations.contains("export type FetchRequest ="));
    assert!(declarations.contains("export type RepresentResponse =\n  |"));
    assert!(!declarations.contains("ItemsItems"));
    assert!(!declarations.contains("Status201Response"));
    assert!(declarations.contains("response: Item;"));
    assert!(declarations.contains("export type Item ="));
    assert!(!declarations.contains("export const"));
    assert!(!declarations.contains("from '../runtime'"));
    let files =
        forklaunch_core::typescript::generate_typescript_http_sdk_files(&plan, &options).unwrap();
    // This equality check belongs to the monorepo: distributed Rust sources do
    // not contain the separate TypeScript framework workspace.
    let runtime = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../framework/universal-sdk/src/portableRuntime.ts");
    if runtime.exists() {
        assert_eq!(
            files["runtime.ts"],
            std::fs::read_to_string(runtime).unwrap()
        );
    }
    if let Some(root) = std::env::var_os("FORKLAUNCH_PORTABLE_SDK_FIXTURE") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("sdk.ts"), declarations).unwrap();
        std::fs::write(
            root.join("openapi.json"),
            forklaunch_core::openapi::generate_portable_openapi_json(&registry).unwrap(),
        )
        .unwrap();
    }
}
