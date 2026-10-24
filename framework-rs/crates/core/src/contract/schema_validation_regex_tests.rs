use super::*;
use serde_json::json;
const KEY: &str = "x-forklaunch-zod-regex";
fn rule(pattern: &str) -> Value {
    json!({KEY:{"version":1,"rules":[{"sourceUnits":pattern.encode_utf16().collect::<Vec<_>>(),"flags":""}]}})
}
#[test]
fn legacy_compiler_rejects_active_demand_not_data() {
    for schema in [
        rule("a"),
        json!({"properties":{"x":rule("a")}}),
        json!({"default":rule("a"),"$ref":"#/default"}),
    ] {
        assert!(compile(&schema).is_err());
    }
    assert!(compile(&json!({"const":rule("a")})).is_ok());
}
#[cfg(not(feature = "zod-regex"))]
#[test]
fn feature_off_checked_rejects_active_demand() {
    assert!(compile_checked(&rule("a")).is_err());
}
#[cfg(feature = "zod-regex")]
mod enabled {
    use super::*;
    fn tiny() -> RegexProfile {
        RegexProfile {
            matching: forklaunch_common::zod_regex::RegexMatchLimits {
                work: 1000,
                ..Default::default()
            },
            ..Default::default()
        }
    }
    #[test]
    fn operational_dominates_boolean_branches() {
        let pattern = rule("^(a+)+$");
        let input = json!(format!("{}!", "a".repeat(40)));
        for schema in [
            json!({"not":pattern}),
            json!({"anyOf":[pattern,true]}),
            json!({"if":pattern,"then":false,"else":true}),
            json!({"oneOf":[pattern,true]}),
            json!({"allOf":[pattern,true]}),
        ] {
            let validator = compile_with_regex_profile(&schema, false, tiny()).unwrap();
            assert_eq!(validator(&input), Err(SchemaValidationFailure::Operational));
        }
        assert_eq!(
            compile_with_regex_profile(&json!({"anyOf":[true,pattern]}), false, tiny()).unwrap()(
                &input
            ),
            Ok(())
        );
    }
    #[test]
    fn contains_cannot_turn_exhaustion_into_validity() {
        let schema = json!({"contains":rule("^(a+)+$")});
        let input = json!([format!("{}!", "a".repeat(40)), "a"]);
        assert_eq!(
            compile_with_regex_profile(&schema, false, tiny()).unwrap()(&input),
            Err(SchemaValidationFailure::Operational)
        );
    }
    #[test]
    fn aggregate_admission_and_work_are_configurable() {
        let schema = json!({"allOf":[rule("a"),rule("b")]});
        assert!(
            compile_with_regex_profile(
                &schema,
                false,
                RegexProfile {
                    rules: 1,
                    ..Default::default()
                }
            )
            .is_err()
        );
        let low = RegexProfile {
            evaluation_work: 12_097_153,
            ..Default::default()
        };
        assert_eq!(
            compile_with_regex_profile(&schema, false, low).unwrap()(&json!("ab")),
            Err(SchemaValidationFailure::Operational)
        );
        assert_eq!(compile_checked(&schema).unwrap()(&json!("ab")), Ok(()));
        assert!(matches!(
            compile_checked(&schema).unwrap()(&json!("a")),
            Err(SchemaValidationFailure::Invalid(_))
        ));
    }
    #[test]
    fn malformed_rules_rejected_before_engine() {
        for value in [
            json!({"version":2,"rules":[]}),
            json!({"version":1,"rules":[{"sourceUnits":[65536],"flags":""}]}),
            json!({"version":1,"rules":[{"sourceUnits":[-1],"flags":""}]}),
            json!({"version":1,"rules":[{"sourceUnits":[0.5],"flags":""}]}),
            json!({"version":1,"rules":[{"sourceUnits":[],"flags":"uv"}]}),
        ] {
            assert!(compile_checked(&json!({KEY:value})).is_err());
        }
    }
    #[test]
    fn scopes_restore_nested_and_unwind_state() {
        let _outer = bounded_regex::Scope::enter(1);
        {
            let _inner = bounded_regex::Scope::enter(0);
            assert!(!bounded_regex::Scope::failed());
        }
        assert!(!bounded_regex::Scope::failed());
        assert!(
            std::panic::catch_unwind(|| {
                let _inner = bounded_regex::Scope::enter(0);
                panic!("test unwind");
            })
            .is_err()
        );
        assert!(!bounded_regex::Scope::failed());
    }
    #[test]
    fn concurrent_checked_reuse_is_isolated() {
        let validator = compile_with_regex_profile(&rule("^(a+)+$"), false, tiny()).unwrap();
        let handles: Vec<_> = (0..8)
            .map(|index| {
                let validator = Arc::clone(&validator);
                std::thread::spawn(move || {
                    for _ in 0..10 {
                        if index % 2 == 0 {
                            assert_eq!(
                                validator(&json!(format!("{}!", "a".repeat(40)))),
                                Err(SchemaValidationFailure::Operational)
                            );
                        } else {
                            assert_eq!(validator(&json!("a")), Ok(()));
                        }
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
    }
}
