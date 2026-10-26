use super::compile;
use serde_json::json;

#[test]
fn reserved_check_capability_follows_schema_positions_and_explicit_refs() {
    let assertion =
        json!({"type":"string","x-forklaunch-zod-checks":{"version":1,"utf16MaxLength":1}});
    for schema in [
        assertion.clone(),
        json!({"type":"object","properties":{"value":assertion}}),
        json!({"$defs":{"Check":assertion},"$ref":"#/$defs/Check"}),
        json!({"default":assertion,"$ref":"#/default"}),
    ] {
        assert_eq!(compile(&schema).is_ok(), cfg!(feature = "zod-checks"));
    }
    // Literal instance data is never interpreted as a schema unless referenced.
    let schema = json!({"const":{"x-forklaunch-zod-checks":{"version":999}},"default":{"x-forklaunch-zod-checks":null}});
    assert!(compile(&schema).unwrap()(&schema["const"]).is_ok());
}
#[cfg(feature = "zod-checks")]
#[test]
fn exact_assertions_reject_malformed_configs_and_preserve_input() {
    for invalid in [
        json!(null),
        json!([]),
        json!({}),
        json!({"version":2}),
        json!({"version":1,"extra":0}),
        json!({"version":1,"utf16MinLength":-1}),
        json!({"version":1,"utf16MaxLength":0.5}),
        json!({"version":1,"multipleOf":null}),
        json!({"version":1,"multipleOf":["nan"]}),
        json!({"version":1,"multipleOf":vec![1;10001]}),
    ] {
        assert!(compile(&json!({"x-forklaunch-zod-checks":invalid})).is_err());
    }
    let schema = json!({"type":"string","x-forklaunch-zod-checks":{"version":1.0,"utf16MinLength":1.0,"utf16MaxLength":1}});
    let validate = compile(&schema).unwrap();
    for (text, allowed) in [("a", true), ("水", true), ("", false), ("😀", false)] {
        let input = json!(text);
        let before = input.clone();
        assert_eq!(validate(&input).is_ok(), allowed);
        assert_eq!(input, before);
    }
    for (divisors, value, allowed) in [
        (json!([0.1]), json!(0.3), true),
        (json!([2, 3]), json!(3), false),
        (json!([-0.1]), json!(0.3), true),
        (json!([0]), json!(0), false),
        (json!(["NaN"]), json!(1), false),
        (json!(["Infinity"]), json!(1), false),
        (json!(["-Infinity"]), json!(1), false),
    ] {
        assert_eq!(compile(&json!({"type":"number","x-forklaunch-zod-checks":{"version":1,"multipleOf":divisors}})).unwrap()(&value).is_ok(),allowed);
    }
}

#[cfg(feature = "zod-checks")]
#[test]
fn format_checks_are_ordered_bounded_and_uri_requires_its_capability() {
    for invalid in [
        json!(null),
        json!({}),
        json!(["url"]),
        json!(["EMAIL"]),
        json!([null]),
        json!(vec!["email"; 10001]),
    ] {
        assert!(
            compile(&json!({"x-forklaunch-zod-checks":{"version":1,"formats":invalid}})).is_err()
        );
    }
    assert!(compile(&json!({"x-forklaunch-zod-checks":{"version":1,"formats":vec!["email";5001],"multipleOf":vec![1;5000]}})).is_err());
    let email =
        compile(&json!({"x-forklaunch-zod-checks":{"version":1,"formats":["email","email"]}}))
            .unwrap();
    assert!(email(&json!("a'b@b-.co")).is_ok());
    assert!(email(&json!("a%b@b.co")).is_err());
    let mixed =
        compile(&json!({"x-forklaunch-zod-checks":{"version":1,"formats":["email","uuid"]}}))
            .unwrap();
    assert!(mixed(&json!("a@b.co")).is_err());
    assert!(mixed(&json!(true)).is_ok());
    let uri = json!({"x-forklaunch-zod-checks":{"version":1,"formats":["uri"]}});
    assert_eq!(compile(&uri).is_ok(), cfg!(feature = "zod-uri"));
    for schema in [json!({"const":uri}), json!({"default":uri})] {
        assert!(compile(&schema).is_ok());
    }
    assert_eq!(
        compile(&json!({"default":uri,"$ref":"#/default"})).is_ok(),
        cfg!(feature = "zod-uri")
    );
}

#[test]
fn array_cardinality_capability_tracks_refs_but_not_literal_data() {
    let assertion = json!({"type":"array","x-forklaunch-zod-checks":{"version":1,"arrayMinItems":[1.5],"arrayMaxItems":[2.5,"NaN"]}});
    for schema in [
        assertion.clone(),
        json!({"$defs":{"A":assertion},"$ref":"#/$defs/A"}),
        json!({"default":assertion,"$ref":"#/default"}),
    ] {
        assert_eq!(compile(&schema).is_ok(), cfg!(feature = "zod-checks"));
    }
    assert!(compile(&json!({"const":assertion})).is_ok());
}
#[cfg(feature = "zod-checks")]
#[test]
fn array_cardinalities_preserve_ieee_and_reject_bad_or_excess_rules() {
    for (minimum, maximum, value, expected) in [
        (json!([1.5]), json!([2.5]), json!([0, 0]), true),
        (json!([1.5]), json!([2.5]), json!([0]), false),
        (json!(["NaN"]), json!(["NaN"]), json!([]), true),
        (json!(["Infinity"]), json!([]), json!([]), false),
        (json!([]), json!(["-Infinity"]), json!([]), false),
        (json!([1e100]), json!([]), json!([]), false),
        (json!([]), json!([1e100]), json!([0]), true),
        (json!([1e100]), json!([]), json!("other type"), true),
    ] {
        let validate=compile(&json!({"x-forklaunch-zod-checks":{"version":1,"arrayMinItems":minimum,"arrayMaxItems":maximum}})).unwrap();
        let before = value.clone();
        assert_eq!(validate(&value).is_ok(), expected);
        assert_eq!(value, before);
    }
    for key in ["arrayMinItems", "arrayMaxItems"] {
        for bad in [
            json!(null),
            json!({}),
            json!([null]),
            json!([true]),
            json!(["nan"]),
            json!(vec![0; 10001]),
        ] {
            assert!(compile(&json!({"x-forklaunch-zod-checks":{"version":1,key:bad}})).is_err());
        }
    }
    assert!(compile(&json!({"x-forklaunch-zod-checks":{"version":1,"arrayMinItems":vec![0;5001],"arrayMaxItems":vec![2;5000]}})).is_err());
}

#[test]
fn extended_utf16_capability_tracks_schema_positions_and_refs() {
    let assertion = json!({"x-forklaunch-zod-checks":{"version":1,"utf16MinLengths":[1.5],"utf16MaxLengths":[2.5,"NaN"]}});
    for schema in [
        assertion.clone(),
        json!({"properties":{"text":assertion}}),
        json!({"$defs":{"Text":assertion},"$ref":"#/$defs/Text"}),
        json!({"default":assertion,"$ref":"#/default"}),
    ] {
        assert_eq!(compile(&schema).is_ok(), cfg!(feature = "zod-checks"));
    }
    assert!(compile(&json!({"const":assertion})).unwrap()(&assertion).is_ok());
}

#[cfg(feature = "zod-checks")]
#[test]
fn extended_utf16_ieee_bounds_preserve_legacy_admission_and_budget() {
    let validate = compile(&json!({"x-forklaunch-zod-checks":{"version":1,"utf16MinLengths":[1.5,"NaN"],"utf16MaxLengths":[2.5]}})).unwrap();
    for (value, allowed) in [
        (json!("a"), false),
        (json!("😀"), true),
        (json!("a😀"), false),
        (json!(null), true),
    ] {
        assert_eq!(validate(&value).is_ok(), allowed);
    }
    for key in ["utf16MinLengths", "utf16MaxLengths"] {
        for invalid in [
            json!(null),
            json!(0),
            json!([null]),
            json!([true]),
            json!(["nan"]),
            json!(vec![0; 10001]),
        ] {
            assert!(
                compile(&json!({"x-forklaunch-zod-checks":{"version":1,key:invalid}})).is_err()
            );
        }
    }
    for key in ["utf16MinLength", "utf16MaxLength"] {
        for invalid in [json!(-1), json!(0.5), json!("Infinity")] {
            assert!(
                compile(&json!({"x-forklaunch-zod-checks":{"version":1,key:invalid}})).is_err()
            );
        }
    }
    assert!(compile(&json!({"x-forklaunch-zod-checks":{"version":1,"utf16MinLengths":vec![0;5001],"arrayMaxItems":vec![2;5000]}})).is_err());
    assert!(compile(&json!({"x-forklaunch-zod-checks":{"version":1,"utf16MinLengths":vec![0;5000],"arrayMaxItems":vec![2;5000]}})).is_ok());
}

#[test]
fn string_predicate_demand_tracks_schema_refs_not_literal_data() {
    let assertion = json!({"x-forklaunch-zod-checks":{"version":1,"stringPredicates":[{"kind":"startsWith","sourceUnits":[55357]}]}});
    for schema in [
        assertion.clone(),
        json!({"allOf":[assertion]}),
        json!({"$defs":{"P":assertion},"$ref":"#/$defs/P"}),
        json!({"default":assertion,"$ref":"#/default"}),
    ] {
        assert_eq!(compile(&schema).is_ok(), cfg!(feature = "zod-checks"));
    }
    assert!(compile(&json!({"const":assertion})).is_ok());
}
#[cfg(feature = "zod-checks")]
#[test]
fn literal_predicates_match_utf16_fragments_and_share_rule_admission() {
    let rule = json!({"kind":"includes","sourceUnits":[56832],"position":1.5});
    let schema = json!({"x-forklaunch-zod-checks":{"version":1,"stringPredicates":[rule]}});
    let validate = compile(&schema).unwrap();
    assert!(validate(&json!("😀")).is_ok());
    assert!(validate(&json!("a")).is_err());
    assert!(validate(&json!(null)).is_ok());
    for units in [
        json!([-1]),
        json!([65536]),
        json!([0.5]),
        json!([null]),
        json!(["1"]),
    ] {
        assert!(compile(&json!({"x-forklaunch-zod-checks":{"version":1,"stringPredicates":[{"kind":"startsWith","sourceUnits":units}]}})).is_err());
    }
    let long = json!({"kind":"endsWith","sourceUnits":vec![97;4096]});
    assert!(
        compile(&json!({"x-forklaunch-zod-checks":{"version":1,"stringPredicates":vec![long;65]}}))
            .is_err()
    );
    assert!(compile(&json!({"x-forklaunch-zod-checks":{"version":1,"stringPredicates":vec![rule;5001],"arrayMaxItems":vec![2;5000]}})).is_err());
}
