use super::*;
use serde_json::json;

#[test]
fn terminal_factories_distinguish_missing_null_and_diagnostics() {
    for (schema, expected) in [
        (ZodJsonSchema::Null, "null"),
        (ZodJsonSchema::Undefined, "undefined"),
        (ZodJsonSchema::Void, "void"),
        (ZodJsonSchema::Never, "never"),
    ] {
        let result = zod_json(None, &schema).unwrap();
        if matches!(schema, ZodJsonSchema::Undefined | ZodJsonSchema::Void) {
            assert_eq!(result, ZodJsonOutcome::Valid(None));
        } else {
            assert_eq!(
                result,
                ZodJsonOutcome::Invalid(vec![ParseError {
                    path: vec![],
                    message: "Required".into()
                }])
            );
        }
        for (input, received) in [
            (Value::Null, "null"),
            (json!(true), "boolean"),
            (json!(0), "number"),
            (json!("x"), "string"),
            (json!([]), "array"),
            (json!({}), "object"),
        ] {
            let result = zod_json_at_path(Some(&input), &schema, &["body"]).unwrap();
            if expected == "null" && input == Value::Null {
                assert_eq!(result, ZodJsonOutcome::Valid(Some(Value::Null)));
            } else {
                assert_eq!(
                    result,
                    ZodJsonOutcome::Invalid(vec![ParseError {
                        path: vec!["body".into()],
                        message: format!("Expected {expected}, received {received}")
                    }])
                );
            }
        }
    }
}
#[test]
fn object_absence_is_omitted_while_supplied_null_is_retained_or_rejected() {
    let schema = ZodJsonSchema::Object(vec![
        ZodJsonField {
            name: "absent",
            schema: ZodJsonSchema::Undefined,
        },
        ZodJsonField {
            name: "void",
            schema: ZodJsonSchema::Void,
        },
        ZodJsonField {
            name: "null",
            schema: ZodJsonSchema::Null,
        },
        ZodJsonField {
            name: "forbidden",
            schema: ZodJsonSchema::Optional(Box::new(ZodJsonSchema::Never)),
        },
    ]);
    assert_eq!(
        zod_json(Some(&json!({"null":null})), &schema),
        Ok(ZodJsonOutcome::Valid(Some(json!({"null":null}))))
    );
    let input = json!({"null":null,"absent":null});
    let original = input.clone();
    assert!(matches!(
        zod_json(Some(&input), &schema),
        Ok(ZodJsonOutcome::Invalid(_))
    ));
    assert_eq!(input, original);
    let fallback = Value::Null;
    let schema = ZodJsonSchema::Default {
        schema: Box::new(ZodJsonSchema::Null),
        value: &fallback,
    };
    assert_eq!(
        zod_json(None, &schema),
        Ok(ZodJsonOutcome::Valid(Some(Value::Null)))
    );
}
#[test]
fn nullish_lowering_preserves_union_order_and_abort_behavior() {
    let schema = ZodJsonSchema::Union(vec![
        ZodJsonSchema::Void,
        ZodJsonSchema::Null,
        ZodJsonSchema::Undefined,
    ]);
    assert_eq!(zod_json(None, &schema), Ok(ZodJsonOutcome::Valid(None)));
    assert_eq!(
        zod_json(Some(&Value::Null), &schema),
        Ok(ZodJsonOutcome::Valid(Some(Value::Null)))
    );
    let ZodJsonOutcome::Invalid(errors) =
        zod_json_at_path(Some(&json!(false)), &schema, &["body"]).unwrap()
    else {
        panic!("expected errors")
    };
    assert_eq!(errors.len(), 3);
    for (index, expected) in ["void", "null", "undefined"].iter().enumerate() {
        assert_eq!(
            errors[index].path,
            vec![
                format!("Union Schema Variant {index}"),
                "body".into(),
                "body".into()
            ]
        );
        assert_eq!(
            errors[index].message,
            format!("Expected {expected}, received boolean")
        );
    }
    let schema = ZodJsonSchema::Union(vec![ZodJsonSchema::Never, ZodJsonSchema::Unknown]);
    assert_eq!(
        zod_json(Some(&json!(false)), &schema),
        Ok(ZodJsonOutcome::Valid(Some(json!(false))))
    );
}
#[test]
fn terminal_record_keys_abort_but_values_still_validate() {
    let input = OrderedJsonDocument::parse(br#"{"z":null,"a":null}"#).unwrap();
    for key in [
        ZodJsonSchema::Null,
        ZodJsonSchema::Void,
        ZodJsonSchema::Undefined,
        ZodJsonSchema::Never,
    ] {
        let schema = ZodJsonSchema::Record {
            key: Box::new(key),
            value: Box::new(ZodJsonSchema::Never),
        };
        let ZodJsonOutcome::Invalid(errors) = zod_json_ordered(&input, &schema).unwrap() else {
            panic!("expected key/value errors")
        };
        assert_eq!(errors.len(), 4);
        assert_eq!(errors[0].path, ["z"]);
        assert_eq!(errors[1].message, "Expected never, received null");
        assert_eq!(errors[2].path, ["a"]);
    }
}
