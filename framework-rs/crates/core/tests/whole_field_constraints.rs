#![cfg(all(feature = "build", feature = "typed-schema"))]
use forklaunch_core::contract::{
    ContractRegistry, HttpMethod, ServiceContract,
    endpoint::{Endpoint, Fields, NoBody, TextBody},
};
#[allow(dead_code)]
struct Query {
    start: Option<String>,
    end: Option<String>,
}
#[allow(dead_code)]
struct Headers {
    start: Option<String>,
    end: Option<String>,
}
#[test]
fn whole_field_constraints_survive_artifact_and_sdk_compilation() {
    let endpoint = Endpoint::<Fields<NoBody, Query, Headers>, TextBody>::new(
        "Range",
        "Range",
        HttpMethod::Get,
        "/range",
        200,
    )
    .unwrap();
    let mut registry = ContractRegistry::for_service(ServiceContract::new("fields", "1"));
    registry.register(endpoint.contract());
    let openapi = forklaunch_core::openapi::generate_openapi_json(&registry);
    let document: serde_json::Value = serde_json::from_str(&openapi).unwrap();
    assert_eq!(
        document["paths"]["/range"]["get"]["x-forklaunch-query-schema"]["dependencies"]["start"][0],
        "end"
    );
    let imported = forklaunch_core::build::openapi_artifact_sources(&openapi, "fields").unwrap();
    assert!(imported.sources.typescript.contains("dependencies"));
    assert!(imported.sources.rust.contains("execute_with_fields"));
    let artifact = registry.into_artifact().to_json();
    let output = forklaunch_core::build::http_artifact_sources(&artifact, "fields").unwrap();
    assert!(output.typescript.contains("dependencies"));
    assert!(output.rust.contains("execute_with_fields"));
    if let Ok(directory) = std::env::var("FORKLAUNCH_WHOLE_FIELDS_DIR") {
        std::fs::write(format!("{directory}/fields.ts"), output.typescript).unwrap();
        std::fs::write(format!("{directory}/fields.rs"), output.rust).unwrap();
    }
}

impl schemars::JsonSchema for Query {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Query".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        serde_json::from_value(serde_json::json!({"type":"object","properties":{"start":{"type":"string"},"end":{"type":"string"}},"additionalProperties":false,"dependencies":{"start":["end"]}})).unwrap()
    }
}
impl schemars::JsonSchema for Headers {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Headers".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        serde_json::from_value(serde_json::json!({"type":"object","properties":{"X-Start":{"type":"string"},"X-End":{"type":"string"}},"additionalProperties":false,"dependencies":{"X-Start":["X-End"]}})).unwrap()
    }
}

#[test]
fn version_groups_keep_branch_specific_request_constraints() {
    use forklaunch_core::contract::{SchemaRef, VersionedContract};
    let endpoint = Endpoint::<Fields<NoBody, Query>, TextBody>::new(
        "Ranges",
        "Ranges",
        HttpMethod::Get,
        "/ranges",
        200,
    )
    .unwrap();
    let mut route = endpoint.contract();
    let first = route.request.clone();
    let mut second = first.clone();
    second.query_schema = Some(SchemaRef::named("ReverseQuery").with_json_schema(serde_json::json!({"type":"object","properties":{"start":{"type":"string"},"end":{"type":"string"}},"additionalProperties":false,"dependencies":{"end":["start"]}}).to_string()));
    let mut first_responses = route.responses.clone();
    let mut second_responses = route.responses.clone();
    first_responses[0].body.as_mut().unwrap().schema.json_schema =
        Some(r#"{"type":"string","const":"first"}"#.into());
    second_responses[0]
        .body
        .as_mut()
        .unwrap()
        .schema
        .json_schema = Some(r#"{"type":"string","const":"second"}"#.into());
    route = route
        .with_version(VersionedContract {
            version: "first".into(),
            request: first,
            responses: first_responses,
        })
        .with_version(VersionedContract {
            version: "second".into(),
            request: second,
            responses: second_responses,
        });
    let mut registry = ContractRegistry::for_service(ServiceContract::new("fields", "1"));
    registry.register(route);
    let output = forklaunch_core::build::http_artifact_sources(
        &registry.into_artifact().to_json(),
        "fields",
    )
    .unwrap();
    assert!(output.typescript.contains("requestObjects"));
    assert!(output.rust.contains("requestObjects"));
    if let Ok(directory) = std::env::var("FORKLAUNCH_WHOLE_FIELDS_DIR") {
        std::fs::write(format!("{directory}/versions.ts"), output.typescript).unwrap();
        std::fs::write(format!("{directory}/versions.rs"), output.rust).unwrap();
    }
}
