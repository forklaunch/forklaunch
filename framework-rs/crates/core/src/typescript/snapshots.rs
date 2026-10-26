//! Reviewed output baselines: these catch drift beyond individual field assertions.
use std::{fs, path::Path};

use forklaunch_core::codegen::{SdkGenerationPlan, SdkTarget};
use forklaunch_core::contract::{
    BodyContract, BodyKind, ContractRegistry, FieldContract, RouteContract, SchemaRef,
    ServiceContract,
};
use forklaunch_core::rust_sdk::{RustSdkOptions, generate_rust_http_sdk_source};
use serde_json::json;

use crate::typescript::{TypeScriptHttpSdkOptions, generate_typescript_http_sdk_source};

fn media(kind: BodyKind, content_type: &str) -> BodyContract {
    BodyContract {
        kind,
        schema: SchemaRef::named("String"),
        content_type: Some(content_type.into()),
    }
}

fn registry() -> ContractRegistry {
    let mut registry = ContractRegistry::for_service(ServiceContract::new("catalog", "1.0.0"));
    let item = BodyContract {
        schema: SchemaRef::named("Item").with_json_schema(
            json!({
                "type":"object", "required":["id","name"], "additionalProperties":false,
                "properties":{
                    "id":{"type":"string"}, "name":{"type":"string","minLength":1},
                    "note":{"type":["string","null"]},
                    "tags":{"type":"array","items":{"type":"string"}}
                }
            })
            .to_string(),
        ),
        ..BodyContract::json("Item")
    };
    registry.register(
        RouteContract::get("Get Item", "Retrieve an item", "/items/:id")
            .with_param(FieldContract::required("id", "String"))
            .with_query(FieldContract::optional("includeDetails", "bool"))
            .with_header(FieldContract::optional("x-request-id", "String"))
            .with_response(200, Some(item.clone()))
            .with_response(200, Some(media(BodyKind::Text, "text/plain")))
            .with_response(200, Some(media(BodyKind::Text, "text/html")))
            .with_response(200, Some(media(BodyKind::Text, "text/csv")))
            .with_response(200, Some(media(BodyKind::Binary, "application/pdf")))
            .with_response(302, None)
            .with_response_header(200, FieldContract::required("etag", "String"))
            .with_response_header(
                200,
                FieldContract {
                    name: "x-count".into(),
                    required: true,
                    schema: SchemaRef::named("Count")
                        .with_json_schema(json!({"type":"integer","minimum":0}).to_string()),
                },
            )
            .with_response_header(200, FieldContract::optional("x-next", "String"))
            .with_response(404, Some(media(BodyKind::Text, "text/plain"))),
    );
    registry.register(
        RouteContract::post("Create Item", "Create an item", "/items")
            .with_request_body(item.clone())
            .with_response(201, Some(item)),
    );
    registry.register(
        RouteContract::get("Watch Items", "Watch typed item events", "/items/events")
            .with_response(
                200,
                Some(BodyContract {
                    kind: BodyKind::ServerSentEvent,
                    schema: SchemaRef::named("ItemEvent").with_json_schema(
                        json!({"type":"object","required":["data"],"additionalProperties":false,
                        "properties":{"data":{"type":"object","required":["id"],
                            "properties":{"id":{"type":"string","minLength":1}},
                            "additionalProperties":false},"id":{"type":"string"}}})
                        .to_string(),
                    ),
                    content_type: Some("text/event-stream".into()),
                }),
            ),
    );
    registry.register(
        RouteContract::get("Native Events", "Native payload SSE", "/items/native-events")
            .with_response(200, Some(BodyContract {
                kind: BodyKind::ServerSentEvent,
                schema: SchemaRef::named("NativePayload").with_json_schema(json!({
                    "x-forklaunch-sse-framing":"payload", "type":"object", "required":["id"],
                    "properties":{"id":{"type":"string","minLength":1}},"additionalProperties":false
                }).to_string()), content_type:Some("text/event-stream".into())
            }))
    );
    registry.register(
        RouteContract::delete("Delete Item", "Delete an item", "/items/:id")
            .with_param(FieldContract::required("id", "String"))
            .with_response(204, None),
    );
    registry
}

fn assert_snapshot(name: &str, actual: &str) {
    let actual = actual
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/typescript/snapshots")
        .join(name);
    if std::env::var_os("FORKLAUNCH_UPDATE_SNAPSHOTS").as_deref() == Some(std::ffi::OsStr::new("1"))
    {
        assert!(
            std::env::var_os("CI").is_none(),
            "CI must never rewrite snapshots"
        );
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &actual).unwrap();
    }
    let expected = fs::read_to_string(&path).expect("Missing reviewed snapshot");
    assert!(
        expected == actual,
        "Generated output differs from {}. Review the diff after running FORKLAUNCH_UPDATE_SNAPSHOTS=1 cargo test -p forklaunch-core --features build,asyncapi-import snapshots",
        path.display()
    );
}

#[test]
fn openapi_and_both_http_sdks_match_reviewed_snapshots() {
    let registry = registry();
    let mut openapi: serde_json::Value =
        serde_json::from_str(&forklaunch_core::openapi::generate_openapi_json(&registry)).unwrap();
    openapi.sort_all_objects();
    assert_eq!(
        openapi["paths"]["/items/{id}"]["get"]["operationId"],
        "getItem"
    );
    assert!(
        openapi["paths"]["/items/{id}"]["delete"]["responses"]["204"]
            .get("content")
            .is_none()
    );
    assert_snapshot(
        "catalog.openapi.json",
        &(serde_json::to_string_pretty(&openapi).unwrap() + "\n"),
    );
    let rust = generate_rust_http_sdk_source(
        &SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry),
        &RustSdkOptions::new("catalog-sdk"),
    )
    .unwrap();
    let typescript = generate_typescript_http_sdk_source(
        &SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry),
        &TypeScriptHttpSdkOptions::new("catalog"),
    )
    .unwrap();
    assert_snapshot("catalog.rust.snap", &rust);
    assert_snapshot("catalog.typescript.snap", &typescript);
}
