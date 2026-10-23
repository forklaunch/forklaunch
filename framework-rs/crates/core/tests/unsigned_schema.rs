#![cfg(all(feature = "typed-schema", feature = "schema-validation"))]
use forklaunch_core::contract::{self, schema_validation};
use serde_json::json;

type Count = usize;
type Revision = u64;

#[contract::schema(unknown_fields = "deny")]
struct Counts {
    #[field(max = 9_007_199_254_740_991)]
    record_count: usize,
    #[field(min = 2, max = 100)]
    alias: Count,
    #[field(max = 20)]
    optional: Option<Revision>,
    #[field(min = -10, max = 10)]
    signed: isize,
}

#[test]
fn bounded_unsigned_schemas_preserve_format_and_authored_limits() {
    let output = contract::schema_value_for::<Counts>();
    for schema in [&output, &contract::codec::input_schema(&output)] {
        let fields = &schema["properties"];
        assert_eq!(fields["record_count"]["format"], "uint");
        assert_eq!(fields["record_count"]["minimum"], 0.0);
        assert_eq!(fields["record_count"]["maximum"], 9_007_199_254_740_991_f64);
        assert_eq!(fields["alias"]["format"], "uint");
        assert_eq!(fields["alias"]["minimum"], 2.0);
        assert_eq!(fields["alias"]["maximum"], 100.0);
        assert_eq!(fields["optional"]["format"], "uint64");
        assert_eq!(fields["optional"]["minimum"], 0.0);
        assert_eq!(fields["optional"]["maximum"], 20.0);
        assert_eq!(fields["signed"]["minimum"], -10.0);
        let validate = schema_validation::compile_checked(schema).unwrap();
        let valid = json!({"record_count":0,"alias":2,"optional":null,"signed":-10});
        assert!(validate(&valid).is_ok());
        for (field, invalid) in [
            ("record_count", -1),
            ("alias", -1),
            ("alias", 1),
            ("alias", 101),
            ("optional", -1),
            ("optional", 21),
        ] {
            let mut value = valid.clone();
            value[field] = invalid.into();
            assert!(validate(&value).is_err(), "{field}: {invalid}");
            assert!(serde_json::from_value::<Counts>(value).is_err());
        }
    }
}

#[test]
fn unsigned_formats_do_not_make_unbounded_wide_integers_portable() {
    #[contract::schema]
    struct Wide {
        value: usize,
    }
    let schema = contract::schema_value_for::<Wide>();
    assert_eq!(schema["properties"]["value"]["minimum"], 0.0);
    assert!(schema["properties"]["value"].get("maximum").is_none());
    #[cfg(feature = "codegen")]
    assert!(forklaunch_core::codegen::compile_schema("Wide", &schema).is_err());
}
