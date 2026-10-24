#![cfg(all(feature = "typed-schema", feature = "schema-validation"))]
use forklaunch_core::contract::{
    self,
    datetime::{DateTime, Utc},
};

#[contract::schema(unknown_fields = "deny")]
#[derive(Debug)]
struct Timestamp {
    created_at: DateTime<Utc>,
}

#[test]
fn utc_dates_preserve_milliseconds_and_reject_loss() {
    let output_schema = contract::schema_value_for::<Timestamp>();
    let input_schema = contract::codec::input_schema(&output_schema);
    let validate_input = contract::schema_validation::compile_checked(&input_schema).unwrap();
    let validate_output = contract::schema_validation::compile_checked(&output_schema).unwrap();
    for input in [
        "2024-02-29T12:34:56Z",
        "2024-02-29T12:34:56.1Z",
        "2024-02-29T12:34:56.12Z",
        "2024-02-29T12:34:56.123Z",
    ] {
        assert!(validate_input(&serde_json::json!({"created_at":input})).is_ok());
        let parsed: Timestamp =
            serde_json::from_value(serde_json::json!({"created_at":input})).unwrap();
        let output = serde_json::to_value(&parsed).unwrap();
        assert_eq!(output["created_at"].as_str().unwrap().len(), 24);
        assert!(validate_output(&output).is_ok());
    }
    for input in [
        "2023-02-29T12:34:56Z",
        "2024-02-29T12:34:60Z",
        "2024-02-29T12:34:56.1234Z",
        "2024-02-29T12:34:56+00:00",
        "2024-02-29t12:34:56z",
        "2024-02-29T12:34:56.Z",
    ] {
        assert!(
            validate_input(&serde_json::json!({"created_at":input})).is_err(),
            "schema {input}"
        );
        assert!(
            serde_json::from_value::<Timestamp>(serde_json::json!({"created_at":input})).is_err(),
            "{input}"
        );
    }
    let precise = DateTime::parse_from_rfc3339("2024-02-29T12:34:56.123456Z")
        .unwrap()
        .with_timezone(&Utc);
    assert!(
        serde_json::to_value(Timestamp {
            created_at: precise
        })
        .is_err()
    );
}

#[test]
fn wire_input_and_canonical_output_differ_explicitly() {
    let output = contract::schema_value_for::<Timestamp>();
    let input = contract::codec::input_schema(&output);
    assert_ne!(input, output);
    assert_eq!(
        input["properties"]["created_at"]["x-forklaunch-date-codec"]["projection"],
        "input"
    );
    assert_eq!(
        output["properties"]["created_at"]["x-forklaunch-date-codec"]["projection"],
        "value"
    );
}

// Only the `build` tests register it.
#[cfg_attr(not(feature = "build"), allow(dead_code))]
#[contract::schema(unknown_fields = "deny")]
struct Calendar {
    entries: Vec<Timestamp>,
    primary: Timestamp,
    ordinary: String,
}

#[cfg(feature = "build")]
#[test]
fn date_assets_are_native_in_both_clients() {
    use forklaunch_core::{
        codegen::{SdkGenerationPlan, SdkTarget},
        contract::{
            ContractRegistry, HttpMethod, ServiceContract,
            endpoint::{Endpoint, JsonBody, NoBody, SseBody},
        },
    };
    let mut registry = ContractRegistry::for_service(ServiceContract::new("dates", "1"));
    registry.register(
        Endpoint::<JsonBody<Calendar>, JsonBody<Calendar>>::new(
            "apply",
            "Apply",
            HttpMethod::Post,
            "/dates",
            200,
        )
        .unwrap()
        .contract(),
    );
    registry.register(
        Endpoint::<NoBody, SseBody<Calendar>>::new(
            "watch",
            "Watch",
            HttpMethod::Get,
            "/dates/watch",
            200,
        )
        .unwrap()
        .contract(),
    );
    registry.register(
        Endpoint::<JsonBody<DateShapes>, JsonBody<DateShapes>>::new(
            "shapes",
            "Shapes",
            HttpMethod::Post,
            "/dates/shapes",
            200,
        )
        .unwrap()
        .contract(),
    );
    let ts = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry),
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("dates"),
    )
    .unwrap();
    let rs = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &SdkGenerationPlan::from_registry(SdkTarget::Rust, &registry),
        &forklaunch_core::rust_sdk::RustSdkOptions::new("dates"),
    )
    .unwrap();
    assert!(ts.contains("created_at: Date"));
    assert!(rs.contains("datetime::DateTime"));
    if let Ok(path) = std::env::var("FORKLAUNCH_DATE_SDK_DIR") {
        let path = std::path::Path::new(&path);
        std::fs::write(path.join("client.ts"), ts).unwrap();
        std::fs::write(path.join("client.rs"), rs).unwrap();
        let shape = contract::schema_value_for::<DateShapes>();
        let socket = forklaunch_core::ws::SocketContract::new("/dates")
            .with_client_message(
                "dates",
                forklaunch_core::ws::SocketSchemaEntry::new(shape.clone()),
            )
            .with_server_message("dates", forklaunch_core::ws::SocketSchemaEntry::new(shape));
        let plan =
            forklaunch_core::codegen::websocket::SocketSdkPlan::from_contract(&socket).unwrap();
        let source = forklaunch_core::rust_sdk::generate_rust_websocket_sdk_source(
            &plan,
            &forklaunch_core::rust_sdk::RustSdkOptions::new("dates"),
        )
        .unwrap();
        std::fs::write(path.join("socket.rs"), source).unwrap();
    }
}

#[cfg(feature = "build")]
#[test]
fn unsupported_parameter_date_transports_are_not_silently_stringified() {
    use forklaunch_core::{
        codegen::{SdkField, SdkGenerationPlan, SdkTarget},
        contract::{
            ContractRegistry, HttpMethod, ServiceContract,
            endpoint::{Endpoint, JsonBody},
        },
    };
    let mut registry = ContractRegistry::for_service(ServiceContract::new("dates", "1"));
    registry.register(
        Endpoint::<JsonBody<Calendar>, JsonBody<Calendar>>::new(
            "apply",
            "Apply",
            HttpMethod::Post,
            "/dates",
            200,
        )
        .unwrap()
        .contract(),
    );
    let original = SdkGenerationPlan::from_registry(SdkTarget::TypeScript, &registry);
    let field = SdkField {
        name: "date".into(),
        schema: "Timestamp".into(),
        json_schema: Some(contract::schema_value_for::<Timestamp>().to_string()),
        required: true,
    };
    for location in ["params", "query", "headers", "response"] {
        let mut plan = original.clone();
        match location {
            "params" => plan.operations[0].request_params.push(field.clone()),
            "query" => plan.operations[0].request_query.push(field.clone()),
            "headers" => plan.operations[0].request_headers.push(field.clone()),
            _ => plan.operations[0].responses[0].headers.push(field.clone()),
        }
        assert!(
            forklaunch_core::typescript::generate_typescript_http_sdk_source(
                &plan,
                &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("dates")
            )
            .is_err(),
            "{location}"
        );
        assert!(
            forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
                &plan,
                &forklaunch_core::rust_sdk::RustSdkOptions::new("dates")
            )
            .is_err(),
            "{location}"
        );
    }
}

use forklaunch_core::contract::{datetime::DateCodec, schema::Optional};
type Stamp = DateTime<Utc>;
#[contract::schema(unknown_fields = "deny")]
struct DateShapes {
    maybe: Option<DateTime<Utc>>,
    #[field(optional)]
    supplied: Optional<DateTime<Utc>>,
    #[field(optional,codec=DateCodec::UtcMilliseconds)]
    nullable: Optional<Option<Stamp>>,
    dates: Vec<Option<DateTime<Utc>>>,
    pair: [DateTime<Utc>; 2],
    #[field(codec=DateCodec::UtcMilliseconds)]
    alias: Stamp,
}
#[test]
fn date_shapes_preserve_absence_null_and_collections() {
    let stamp = "2024-02-29T12:34:56.123Z";
    let base = serde_json::json!({"dates":[stamp,null],"pair":[stamp,stamp],"alias":stamp});
    let schema = contract::schema_value_for::<DateShapes>();
    let input = contract::codec::input_schema(&schema);
    let validate = contract::schema_validation::compile_checked(&input).unwrap();
    let parsed: DateShapes = serde_json::from_value(base.clone()).unwrap();
    assert!(parsed.maybe.is_none());
    assert!(parsed.supplied.is_missing());
    assert!(parsed.nullable.is_missing());
    assert_eq!(
        parsed.dates[0].unwrap().timestamp_millis(),
        1_709_210_096_123
    );
    let serialized = serde_json::to_value(parsed).unwrap();
    assert!(serialized.get("supplied").is_none());
    assert!(serialized.get("nullable").is_none());
    assert_eq!(serialized["maybe"], serde_json::Value::Null);
    assert!(validate(&base).is_ok());
    for (key, value, accepted) in [
        ("supplied", serde_json::Value::Null, false),
        ("supplied", serde_json::json!(stamp), true),
        ("nullable", serde_json::Value::Null, true),
        ("nullable", serde_json::json!(stamp), true),
        ("maybe", serde_json::Value::Null, true),
        (
            "dates",
            serde_json::json!(["2024-02-29T12:34:56.1234Z"]),
            false,
        ),
        ("pair", serde_json::json!([stamp]), false),
        ("pair", serde_json::json!([stamp, null]), false),
    ] {
        let mut body = base.clone();
        body[key] = value;
        let parsed = serde_json::from_value::<DateShapes>(body.clone());
        assert_eq!(parsed.is_ok(), accepted, "native {key}: {body}");
        assert_eq!(validate(&body).is_ok(), accepted, "schema {key}: {body}");
        if key == "nullable" && accepted {
            assert_eq!(
                serde_json::to_value(parsed.unwrap()).unwrap()[key],
                body[key]
            );
        }
    }
}

#[contract::schema]
struct UnboundAlias {
    date: Stamp,
}
#[contract::schema]
struct NestedUnboundAlias {
    nested: UnboundAlias,
}
#[test]
fn native_alias_policy_is_not_lost_to_transitive_derive_features() {
    // Test-only chrono/serde + Schemars/chrono feature unification makes raw alias
    // derives available; the portable schema must still reject the missing policy.
    let schema = contract::schema_value_for::<NestedUnboundAlias>();
    assert!(
        schema
            .to_string()
            .contains("x-forklaunch-unbound-native-date")
    );
    let validate = contract::schema_validation::compile_checked(&schema).unwrap();
    assert!(validate(&serde_json::json!({"nested":{"date":"2024-02-29T12:34:56.123Z"}})).is_err());
    #[cfg(feature = "build")]
    {
        assert!(
            forklaunch_core::codegen::compile_schema("UnsafeAlias", &schema)
                .unwrap_err()
                .to_string()
                .contains("requires #[field(codec")
        );
        let explicit = serde_json::json!({"type":"string","format":"date-time"});
        assert_eq!(
            forklaunch_core::codegen::compile_schema("StringDate", &explicit)
                .unwrap()
                .typescript_type,
            "string"
        );
    }
}

#[cfg(feature = "build")]
#[test]
fn unsupported_date_composition_graphs_fail_before_emission() {
    let leaf = contract::schema_value_for::<Timestamp>();
    let intersection = serde_json::json!({"allOf":[leaf.clone()]});
    assert!(forklaunch_core::codegen::compile_schema("Intersection", &intersection).is_err());
    let siblings = serde_json::json!({"$schema":"https://json-schema.org/draft/2020-12/schema","$defs":{"Timestamp":leaf},"$ref":"#/$defs/Timestamp","additionalProperties":false});
    assert!(forklaunch_core::codegen::compile_schema("Siblings", &siblings).is_err());
}
