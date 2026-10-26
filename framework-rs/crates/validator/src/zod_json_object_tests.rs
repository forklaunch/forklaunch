use super::*;
use serde_json::json;
fn policy(unknown: ZodJsonUnknownKeys, catchall: Option<ZodJsonSchema<'_>>) -> ZodJsonSchema<'_> {
    ZodJsonSchema::ObjectWithPolicy {
        fields: vec![ZodJsonField {
            name: "field",
            schema: ZodJsonSchema::String(vec![]),
        }],
        unknown,
        catchall: catchall.map(Box::new),
    }
}
#[test]
fn never_sentinel_is_unwrapped_only_and_active_catchall_overrides_policy() {
    let input = json!({"field":"ok","extra":"2"});
    for unknown in [
        ZodJsonUnknownKeys::Strip,
        ZodJsonUnknownKeys::Strict,
        ZodJsonUnknownKeys::Passthrough,
    ] {
        assert_eq!(
            zod_json(
                Some(&input),
                &policy(unknown, Some(ZodJsonSchema::Number(vec![])))
            ),
            Ok(ZodJsonOutcome::Valid(Some(
                json!({"field":"ok","extra":2.0})
            )))
        );
        assert!(matches!(
            zod_json(
                Some(&input),
                &policy(
                    unknown,
                    Some(ZodJsonSchema::Optional(Box::new(ZodJsonSchema::Never)))
                )
            ),
            Ok(ZodJsonOutcome::Invalid(_))
        ));
    }
    assert_eq!(
        zod_json(
            Some(&input),
            &policy(ZodJsonUnknownKeys::Passthrough, Some(ZodJsonSchema::Never))
        ),
        zod_json(Some(&input), &policy(ZodJsonUnknownKeys::Passthrough, None))
    );
    assert_eq!(
        zod_json(
            Some(&input),
            &policy(ZodJsonUnknownKeys::Strip, Some(ZodJsonSchema::Never))
        ),
        Ok(ZodJsonOutcome::Valid(Some(json!({"field":"ok"}))))
    );
}
#[test]
fn catchall_declared_diagnostics_precede_ordered_extra_diagnostics_and_proto_validates() {
    let doc = OrderedJsonDocument::parse(
        br#"{"z":"bad","2":"bad","field":0,"a":"bad","__proto__":"bad"}"#,
    )
    .unwrap();
    let ZodJsonOutcome::Invalid(errors) = zod_json_ordered_at_path(
        &doc,
        &policy(
            ZodJsonUnknownKeys::Strict,
            Some(ZodJsonSchema::Number(vec![])),
        ),
        &["body"],
    )
    .unwrap() else {
        panic!("expected errors")
    };
    let paths: Vec<_> = errors.iter().map(|error| error.path.clone()).collect();
    assert_eq!(
        paths,
        vec![
            vec!["body", "field"],
            vec!["body", "2"],
            vec!["body", "z"],
            vec!["body", "a"],
            vec!["body", "__proto__"]
        ]
    );
    let doc =
        OrderedJsonDocument::parse(br#"{"field":"ok","__proto__":"2","constructor":"3"}"#).unwrap();
    let ZodJsonOutcome::Valid(Some(value)) = zod_json_ordered(
        &doc,
        &policy(
            ZodJsonUnknownKeys::Strip,
            Some(ZodJsonSchema::Number(vec![])),
        ),
    )
    .unwrap() else {
        panic!("expected success")
    };
    assert!(value.get("__proto__").is_none());
    assert_eq!(value["constructor"].as_f64(), Some(3.0));
    assert!(doc.value().get("__proto__").is_some());
}
#[test]
fn ordinary_success_needs_no_order_and_unselected_errors_can_be_discarded() {
    let input = json!({"field":"ok","z":"2","a":"3"});
    assert!(matches!(
        zod_json(
            Some(&input),
            &policy(
                ZodJsonUnknownKeys::Strip,
                Some(ZodJsonSchema::Number(vec![]))
            )
        ),
        Ok(ZodJsonOutcome::Valid(_))
    ));
    let input = json!({"field":0,"z":"bad","a":"bad"});
    let schema = policy(
        ZodJsonUnknownKeys::Strip,
        Some(ZodJsonSchema::Number(vec![])),
    );
    assert_eq!(
        zod_json(Some(&input), &schema),
        Err(ZodJsonUnsupported::ObjectOrderUnavailable)
    );
    let union = ZodJsonSchema::Union(vec![schema, ZodJsonSchema::Unknown]);
    assert!(matches!(
        zod_json(Some(&input), &union),
        Ok(ZodJsonOutcome::Valid(_))
    ));
}
#[test]
fn passthrough_preserves_finite_numeric_values_and_omits_proto() {
    let doc = OrderedJsonDocument::parse(
        br#"{"field":"ok","__proto__":"bad","z":-0,"a":9007199254740993}"#,
    )
    .unwrap();
    let ZodJsonOutcome::Valid(Some(value)) =
        zod_json_ordered(&doc, &policy(ZodJsonUnknownKeys::Passthrough, None)).unwrap()
    else {
        panic!("expected success")
    };
    assert!(value.get("__proto__").is_none());
    assert!(value["z"].as_f64().unwrap().is_sign_negative());
    assert_eq!(value["a"].as_f64(), Some(9_007_199_254_740_992.0));
}
#[test]
fn catchall_schema_and_repeated_output_are_resource_bounded() {
    let mut inner = ZodJsonSchema::Unknown;
    for _ in 0..70 {
        inner = ZodJsonSchema::Optional(Box::new(inner));
    }
    assert_eq!(
        zod_json(None, &policy(ZodJsonUnknownKeys::Strip, Some(inner))),
        Err(ZodJsonUnsupported::SchemaDepth)
    );
    let large = json!("x".repeat(1024 * 1024));
    let catchall = ZodJsonSchema::Object(vec![ZodJsonField {
        name: "default",
        schema: ZodJsonSchema::Default {
            schema: Box::new(ZodJsonSchema::Unknown),
            value: &large,
        },
    }]);
    let mut input = Map::new();
    input.insert("field".into(), json!("ok"));
    for index in 0..100 {
        input.insert(format!("key{index}"), json!({}));
    }
    assert_eq!(
        zod_json(
            Some(&Value::Object(input)),
            &policy(ZodJsonUnknownKeys::Strip, Some(catchall))
        ),
        Err(ZodJsonUnsupported::EvaluationBytes)
    );
}
