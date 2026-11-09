#![cfg(all(feature = "typed-schema", feature = "schema-validation"))]
use forklaunch_core::contract::{schema, schema::Patch, schema_validation::compile_checked};
use serde_json::{Value, json};

#[schema(rename_all = "camelCase", unknown_fields = "deny")]
#[derive(Debug, PartialEq)]
struct Profile {
    #[field(min_length = 1, max_length = 4)]
    label: String,
    #[field(patch)]
    alias: Patch<String>,
}

#[schema(rename_all = "camelCase", unknown_fields = "deny")]
#[derive(Debug, PartialEq)]
struct Update {
    expected_revision: u32,
    #[field(patch)]
    excerpt: Patch<String>,
    #[field(patch)]
    score: Patch<u32>,
    #[field(patch)]
    profile: Patch<Profile>,
}

fn fixtures() -> Vec<Value> {
    vec![
        json!({"expectedRevision":1}),
        json!({"expectedRevision":1,"excerpt":null}),
        json!({"expectedRevision":1,"excerpt":"hello"}),
        json!({"expectedRevision":1,"excerpt":7}),
        json!({"expectedRevision":1,"score":null}),
        json!({"expectedRevision":1,"score":4_294_967_295_u64}),
        json!({"expectedRevision":1,"score":4_294_967_296_u64}),
        json!({"expectedRevision":1,"score":-1}),
        json!({"expectedRevision":1,"score":1.5}),
        json!({"expectedRevision":1,"profile":null}),
        json!({"expectedRevision":1,"profile":{"label":"hi"}}),
        json!({"expectedRevision":1,"profile":{"label":"hi","alias":null}}),
        json!({"expectedRevision":1,"profile":{"label":"hi","alias":"ok"}}),
        json!({"expectedRevision":1,"profile":{"label":"hi","alias":false}}),
        json!({"expectedRevision":1,"profile":{"label":""}}),
        json!({"expectedRevision":1,"profile":{"label":"longer"}}),
        json!({"expectedRevision":1,"profile":{"label":"hi","unknown":1}}),
        json!({"expectedRevision":1,"profile":{}}),
        json!({"expectedRevision":1,"unknown":1}),
        json!({}),
        json!({"expectedRevision":null}),
        json!({"expectedRevision":-1}),
        json!({"expectedRevision":4_294_967_296_u64}),
    ]
}

#[test]
fn patch_preserves_presence_and_matches_published_schema() {
    let schema = forklaunch_core::contract::schema_value_for::<Update>();
    let validator = compile_checked(&schema).unwrap();
    assert_eq!(schema["required"], json!(["expectedRevision"]));
    assert!(schema["properties"]["excerpt"].get("default").is_none());
    let results = fixtures()
        .into_iter()
        .map(|value| {
            let parsed = serde_json::from_value::<Update>(value.clone());
            assert_eq!(
                parsed.is_ok(),
                validator(&value).is_ok(),
                "schema/runtime mismatch for {value}"
            );
            if let Ok(ref parsed) = parsed {
                assert_eq!(serde_json::to_value(parsed).unwrap(), value);
            }
            json!({"input":value,"accepted":parsed.is_ok()})
        })
        .collect::<Vec<_>>();
    let empty: Update = serde_json::from_value(json!({"expectedRevision":1})).unwrap();
    assert_eq!(empty.excerpt, Patch::Keep);
    let clear: Update =
        serde_json::from_value(json!({"expectedRevision":1,"excerpt":null})).unwrap();
    assert_eq!(clear.excerpt, Patch::Clear);
    let set: Update = serde_json::from_value(json!({"expectedRevision":1,"excerpt":"hi"})).unwrap();
    assert_eq!(set.excerpt, Patch::Set("hi".into()));
    if let Ok(path) = std::env::var("FORKLAUNCH_PATCH_FIXTURE") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&json!({"schema":schema,"cases":results})).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn patch_invalid_constructed_values_fail_before_serialization() {
    assert!(serde_json::to_value(Patch::<String>::Keep).is_err());
    assert!(serde_json::to_value(Patch::Set(None::<String>)).is_err());
    assert_eq!(
        serde_json::to_value(Patch::<String>::Clear).unwrap(),
        Value::Null
    );
    let input = Update {
        expected_revision: 1,
        excerpt: Patch::Keep,
        score: Patch::Keep,
        profile: Patch::Set(Profile {
            label: String::new(),
            alias: Patch::Keep,
        }),
    };
    assert!(serde_json::to_value(input).is_err());
}

#[cfg(feature = "build")]
#[test]
fn patch_generates_optional_nullable_client_members() {
    let schema = forklaunch_core::contract::schema_value_for::<Update>();
    let models = forklaunch_core::codegen::compile_schema("Update", &schema).unwrap();
    let rust =
        forklaunch_core::codegen::render_rust_models(&models.rust_models, &models.rust_aliases)
            .unwrap();
    let typescript = format!(
        "{}\nexport type WireUpdate = {};\n",
        models
            .typescript_definitions
            .iter()
            .map(|(n, t)| format!("export type {n} = {t};"))
            .collect::<Vec<_>>()
            .join("\n"),
        models.typescript_type
    );
    assert!(typescript.contains("excerpt?:") && typescript.contains("null"));
    assert!(rust.contains("deserialize_optional_field"));
    if let Ok(path) = std::env::var("FORKLAUNCH_PATCH_SDK_DIR") {
        let path = std::path::Path::new(&path);
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("models.rs"), rust).unwrap();
        std::fs::write(path.join("models.ts"), typescript).unwrap();
    }
}

#[schema]
struct OptionalValue {
    #[field(patch)]
    value: Patch<Option<String>>,
}

#[test]
fn nullable_inner_values_do_not_collapse_set_into_clear() {
    let clear: OptionalValue = serde_json::from_value(json!({"value": null})).unwrap();
    assert_eq!(clear.value, Patch::Clear);
    let set: OptionalValue = serde_json::from_value(json!({"value": "hi"})).unwrap();
    assert_eq!(set.value, Patch::Set(Some("hi".into())));
    assert!(
        serde_json::to_value(OptionalValue {
            value: Patch::Set(None)
        })
        .is_err()
    );
    assert_eq!(
        serde_json::to_value(OptionalValue { value: Patch::Keep }).unwrap(),
        json!({})
    );
}

#[schema]
struct SmallIntegers {
    byte: u8,
    short: u16,
    unsigned: u32,
    signed_byte: i8,
    signed_short: i16,
    signed: i32,
}

#[test]
fn small_integer_schema_limits_match_native_decoding() {
    let validate =
        compile_checked(&forklaunch_core::contract::schema_value_for::<SmallIntegers>()).unwrap();
    let baseline =
        json!({"byte":0,"short":0,"unsigned":0,"signed_byte":0,"signed_short":0,"signed":0});
    for (field, minimum, maximum) in [
        ("byte", 0_i64, 255_i64),
        ("short", 0, 65_535),
        ("unsigned", 0, 4_294_967_295),
        ("signed_byte", -128, 127),
        ("signed_short", -32_768, 32_767),
        ("signed", -2_147_483_648, 2_147_483_647),
    ] {
        for (value, accepted) in [
            (minimum - 1, false),
            (minimum, true),
            (maximum, true),
            (maximum + 1, false),
        ] {
            let mut input = baseline.clone();
            input[field] = value.into();
            assert_eq!(
                serde_json::from_value::<SmallIntegers>(input.clone()).is_ok(),
                accepted
            );
            assert_eq!(
                validate(&input).is_ok(),
                accepted,
                "schema mismatch for {field}={value}"
            );
        }
    }
}
