use super::super::{ZodStringCheck, ZodStringRule, ZodStringStep};
use super::*;
use serde_json::json;
fn record<'a>(key: ZodJsonSchema<'a>, value: ZodJsonSchema<'a>) -> ZodJsonSchema<'a> {
    ZodJsonSchema::Record {
        key: Box::new(key),
        value: Box::new(value),
    }
}
fn trim() -> ZodJsonSchema<'static> {
    ZodJsonSchema::String(vec![ZodStringCheck {
        rule: ZodStringRule::Step(ZodStringStep::Trim),
        message: None,
    }])
}
#[test]
fn record_validates_key_then_value_even_when_key_aborts() {
    let document = OrderedJsonDocument::parse(br#"{"z":"bad","a":"bad","2":"bad"}"#).unwrap();
    let schema = record(
        ZodJsonSchema::Literal(ZodJsonLiteral::String("allowed")),
        ZodJsonSchema::Number(vec![]),
    );
    let ZodJsonOutcome::Invalid(errors) =
        zod_json_ordered_at_path(&document, &schema, &["body"]).unwrap()
    else {
        panic!("expected errors")
    };
    let paths: Vec<_> = errors.iter().map(|error| error.path.clone()).collect();
    assert_eq!(
        paths,
        vec![
            vec!["body", "2"],
            vec!["body", "2"],
            vec!["body", "z"],
            vec!["body", "z"],
            vec!["body", "a"],
            vec!["body", "a"]
        ]
    );
    assert!(errors[0].message.contains("Invalid literal"));
    assert_eq!(errors[1].message, "Expected number, received nan");
}
#[test]
fn record_collisions_follow_original_keys_and_proto_values_still_validate() {
    for (raw, expected) in [
        (
            r#"{" a ":"1","a":"2","__proto__":"3","constructor":"4"}"#,
            2.0,
        ),
        (r#"{"a":"2"," a ":"1"}"#, 1.0),
    ] {
        let document = OrderedJsonDocument::parse(raw.as_bytes()).unwrap();
        let schema = record(trim(), ZodJsonSchema::Number(vec![]));
        let ZodJsonOutcome::Valid(Some(output)) = zod_json_ordered(&document, &schema).unwrap()
        else {
            panic!("expected success")
        };
        assert_eq!(output["a"].as_f64(), Some(expected));
        assert!(output.get("__proto__").is_none());
    }
    let document = OrderedJsonDocument::parse(br#"{" __proto__ ":"bad"}"#).unwrap();
    assert!(matches!(
        zod_json_ordered(&document, &record(trim(), ZodJsonSchema::Number(vec![]))),
        Ok(ZodJsonOutcome::Invalid(_))
    ));
}
#[test]
fn scalar_keys_use_javascript_property_conversion_and_wrappers_delegate() {
    let document = OrderedJsonDocument::parse(br#"{"-0":"1","0":"2","1e21":"3"}"#).unwrap();
    let schema = record(ZodJsonSchema::Number(vec![]), ZodJsonSchema::Number(vec![]));
    let ZodJsonOutcome::Valid(Some(output)) = zod_json_ordered(&document, &schema).unwrap() else {
        panic!("number keys")
    };
    assert_eq!(output["0"].as_f64(), Some(1.0));
    assert_eq!(output["1e+21"].as_f64(), Some(3.0));
    let document = OrderedJsonDocument::parse(br#"{"TRUE":1,"false":2}"#).unwrap();
    let schema = record(ZodJsonSchema::Boolean, ZodJsonSchema::Unknown);
    let ZodJsonOutcome::Valid(Some(output)) = zod_json_ordered(&document, &schema).unwrap() else {
        panic!("boolean keys")
    };
    assert_eq!(output["true"].as_f64(), Some(1.0));
    let unused = json!("unused");
    let inner = ZodJsonSchema::Nullable(Box::new(ZodJsonSchema::Unknown));
    let key = ZodJsonSchema::Default {
        schema: Box::new(ZodJsonSchema::Optional(Box::new(inner))),
        value: &unused,
    };
    assert!(matches!(
        zod_json_ordered(&document, &record(key, ZodJsonSchema::Unknown)),
        Ok(ZodJsonOutcome::Valid(_))
    ));
    let schema = record(
        ZodJsonSchema::Array {
            element: Box::new(ZodJsonSchema::Unknown),
            checks: vec![],
        },
        ZodJsonSchema::String(vec![]),
    );
    let ZodJsonOutcome::Invalid(errors) = zod_json_ordered(&document, &schema).unwrap() else {
        panic!("container key abort")
    };
    assert_eq!(errors.len(), 4);
}
#[test]
fn ordinary_values_defer_error_order_but_cannot_invent_valid_collision_winners() {
    let schema = record(trim(), ZodJsonSchema::Number(vec![]));
    assert!(matches!(
        zod_json(Some(&json!({"z":"1","a":"2"})), &schema),
        Ok(ZodJsonOutcome::Valid(_))
    ));
    let invalid = json!({"z":"bad","a":"bad"});
    assert_eq!(
        zod_json(Some(&invalid), &schema),
        Err(ZodJsonUnsupported::ObjectOrderUnavailable)
    );
    assert!(matches!(
        zod_json(
            Some(&invalid),
            &ZodJsonSchema::Union(vec![schema.clone(), ZodJsonSchema::Unknown])
        ),
        Ok(ZodJsonOutcome::Valid(_))
    ));
    let collision = json!({" a ":"1","a":"2"});
    assert_eq!(
        zod_json(
            Some(&collision),
            &ZodJsonSchema::Union(vec![schema.clone(), ZodJsonSchema::Unknown])
        ),
        Err(ZodJsonUnsupported::ObjectOrderUnavailable)
    );
    let invalid_collision = json!({" a ":"bad","a":"2"});
    assert!(matches!(
        zod_json(
            Some(&invalid_collision),
            &ZodJsonSchema::Union(vec![schema, ZodJsonSchema::Unknown])
        ),
        Ok(ZodJsonOutcome::Valid(_))
    ));
    let known = json!({"1":"1","01":"2","extra":"3"});
    let schema = record(
        ZodJsonSchema::Union(vec![ZodJsonSchema::Number(vec![]), ZodJsonSchema::Unknown]),
        ZodJsonSchema::Number(vec![]),
    );
    let ZodJsonOutcome::Valid(Some(output)) = zod_json(Some(&known), &schema).unwrap() else {
        panic!("known index/nonindex order")
    };
    assert_eq!(output["1"].as_f64(), Some(2.0));
}
#[test]
fn record_repeated_defaults_and_key_checks_consume_shared_work_allowance() {
    let fallback = json!("x".repeat(1024 * 1024));
    let schema = record(
        ZodJsonSchema::Unknown,
        ZodJsonSchema::Object(vec![ZodJsonField {
            name: "default",
            schema: ZodJsonSchema::Default {
                schema: Box::new(ZodJsonSchema::String(vec![])),
                value: &fallback,
            },
        }]),
    );
    let input = Value::Object(
        (0..100)
            .map(|index| (index.to_string(), json!({})))
            .collect(),
    );
    assert_eq!(
        zod_json(Some(&input), &schema),
        Err(ZodJsonUnsupported::EvaluationBytes)
    );
    let checks = vec![
        ZodStringCheck {
            rule: ZodStringRule::Step(ZodStringStep::Min(0)),
            message: None
        };
        1000
    ];
    let schema = record(ZodJsonSchema::String(checks), ZodJsonSchema::Unknown);
    let input = Value::Object(
        [("k".repeat(1024 * 1024), Value::Null)]
            .into_iter()
            .collect(),
    );
    assert_eq!(
        zod_json(Some(&input), &schema),
        Err(ZodJsonUnsupported::EvaluationBytes)
    );
}
#[test]
fn record_keeps_explicit_nonfinite_key_boundary() {
    let input = json!({"Infinity":1});
    assert_eq!(
        zod_json(
            Some(&input),
            &record(ZodJsonSchema::Number(vec![]), ZodJsonSchema::Unknown)
        ),
        Err(ZodJsonUnsupported::NonFiniteNumber)
    );
}
