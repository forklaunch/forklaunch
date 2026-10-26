use super::*;
use serde_json::json;

#[test]
fn defaults_preserve_absence_order_and_null() {
    let fallback = json!("default");
    let optional_default = ZodJsonSchema::Optional(Box::new(ZodJsonSchema::Default {
        schema: Box::new(ZodJsonSchema::String(vec![])),
        value: &fallback,
    }));
    assert_eq!(
        zod_json(None, &optional_default),
        Ok(ZodJsonOutcome::Valid(None))
    );
    let default_optional = ZodJsonSchema::Default {
        schema: Box::new(ZodJsonSchema::Optional(Box::new(ZodJsonSchema::String(
            vec![],
        )))),
        value: &fallback,
    };
    assert_eq!(
        zod_json(None, &default_optional),
        Ok(ZodJsonOutcome::Valid(Some(fallback.clone())))
    );
    assert!(matches!(
        zod_json(Some(&Value::Null), &default_optional),
        Ok(ZodJsonOutcome::Invalid(_))
    ));
}

#[test]
fn preflight_rejects_depth_and_width_before_evaluation() {
    let mut schema = ZodJsonSchema::Unknown;
    for _ in 0..65 {
        schema = ZodJsonSchema::Optional(Box::new(schema));
    }
    assert_eq!(
        zod_json(None, &schema),
        Err(ZodJsonUnsupported::SchemaDepth)
    );
    let mut input = Value::Null;
    for _ in 0..65 {
        input = Value::Array(vec![input]);
    }
    assert_eq!(
        zod_json(Some(&input), &ZodJsonSchema::Unknown),
        Err(ZodJsonUnsupported::InputDepth)
    );
    let input = Value::Array(vec![Value::Null; MAX_NODES]);
    assert_eq!(
        zod_json(Some(&input), &ZodJsonSchema::Unknown),
        Err(ZodJsonUnsupported::InputNodes)
    );
    let schema = ZodJsonSchema::String(vec![
        ZodStringCheck {
            rule: super::super::ZodStringRule::Step(super::super::ZodStringStep::Trim),
            message: None
        };
        MAX_NODES
    ]);
    assert_eq!(
        zod_json(None, &schema),
        Err(ZodJsonUnsupported::SchemaNodes)
    );
}

#[test]
fn schema_and_evaluation_admission_are_distinct_from_parse_errors() {
    let schema = ZodJsonSchema::Object(vec![
        ZodJsonField {
            name: "a",
            schema: ZodJsonSchema::Unknown,
        },
        ZodJsonField {
            name: "a",
            schema: ZodJsonSchema::Unknown,
        },
    ]);
    assert_eq!(
        zod_json(None, &schema),
        Err(ZodJsonUnsupported::DuplicateField)
    );
    let schema = ZodJsonSchema::Object(vec![ZodJsonField {
        name: "toString",
        schema: ZodJsonSchema::Unknown,
    }]);
    assert_eq!(
        zod_json(None, &schema),
        Err(ZodJsonUnsupported::PrototypeField)
    );
    let mut element = ZodJsonSchema::Unknown;
    for _ in 0..60 {
        element = ZodJsonSchema::Optional(Box::new(element));
    }
    let schema = ZodJsonSchema::Array {
        element: Box::new(element),
        checks: vec![],
    };
    assert_eq!(
        zod_json(Some(&json!(vec![0; 2000])), &schema),
        Err(ZodJsonUnsupported::EvaluationNodes)
    );
}

#[test]
fn status_retains_dirty_vs_aborted_for_future_composition() {
    let schema = ZodJsonSchema::String(vec![ZodStringCheck {
        rule: super::super::ZodStringRule::Step(super::super::ZodStringStep::Min(2)),
        message: None,
    }]);
    let mut remaining = Budget::new();
    assert!(matches!(
        evaluate(Some(&json!("x")), &schema, &mut remaining)
            .unwrap()
            .status,
        Status::Dirty
    ));
    assert!(matches!(
        evaluate(Some(&json!(null)), &schema, &mut remaining)
            .unwrap()
            .status,
        Status::Aborted
    ));
}

#[test]
fn numeric_keys_order_errors_and_unknown_fields_strip_atomically() {
    let schema = ZodJsonSchema::Object(vec![
        ZodJsonField {
            name: "10",
            schema: ZodJsonSchema::String(vec![]),
        },
        ZodJsonField {
            name: "2",
            schema: ZodJsonSchema::String(vec![]),
        },
        ZodJsonField {
            name: "a",
            schema: ZodJsonSchema::String(vec![]),
        },
    ]);
    let ZodJsonOutcome::Invalid(errors) = zod_json(Some(&json!({})), &schema).unwrap() else {
        panic!("expected diagnostics");
    };
    assert_eq!(
        errors
            .iter()
            .map(|e| e.path[0].as_str())
            .collect::<Vec<_>>(),
        ["2", "10", "a"]
    );
    let input = json!({"10":"x","2":"y","a":"z","extra":true});
    assert_eq!(
        zod_json(Some(&input), &schema),
        Ok(ZodJsonOutcome::Valid(Some(
            json!({"10":"x","2":"y","a":"z"})
        )))
    );
    assert_eq!(input["extra"], true);
}

#[test]
fn repeated_container_checks_consume_evaluation_budget_before_application() {
    let string_check = ZodStringCheck {
        rule: super::super::ZodStringRule::Step(super::super::ZodStringStep::Trim),
        message: None,
    };
    let string = ZodJsonSchema::String(vec![string_check; 1000]);
    let number = ZodJsonSchema::Number(vec![
        ZodNumberCheck {
            step: super::super::ZodNumberStep::Finite,
            message: None,
        };
        1000
    ]);
    let nested_array = ZodJsonSchema::Array {
        element: Box::new(ZodJsonSchema::Unknown),
        checks: vec![ZodArrayStep::Min(0); 1000],
    };
    for (element, value) in [
        (string, json!("x")),
        (number, json!(1)),
        (nested_array, json!([])),
    ] {
        let schema = ZodJsonSchema::Array {
            element: Box::new(element),
            checks: vec![],
        };
        let input = Value::Array(vec![value; 200]);
        assert!(admit_schema(&schema).is_ok());
        assert!(admit_value(&input).is_ok());
        assert_eq!(
            zod_json(Some(&input), &schema),
            Err(ZodJsonUnsupported::EvaluationNodes)
        );
    }
}

#[test]
fn aggregate_byte_admission_and_repeated_default_output_are_bounded() {
    let oversized = json!("x".repeat(MAX_ADMISSION_BYTES + 1));
    assert_eq!(
        zod_json(Some(&oversized), &ZodJsonSchema::Unknown),
        Err(ZodJsonUnsupported::InputBytes)
    );
    let default = json!("x".repeat(1024 * 1024));
    for leaf in [ZodJsonSchema::String(vec![]), ZodJsonSchema::Unknown] {
        let schema = ZodJsonSchema::Array {
            element: Box::new(ZodJsonSchema::Object(vec![ZodJsonField {
                name: "payload",
                schema: ZodJsonSchema::Default {
                    schema: Box::new(leaf),
                    value: &default,
                },
            }])),
            checks: vec![],
        };
        let input = Value::Array(vec![json!({}); 100]);
        assert!(admit_schema(&schema).is_ok());
        assert!(admit_value(&input).is_ok());
        assert_eq!(
            zod_json(Some(&input), &schema),
            Err(ZodJsonUnsupported::EvaluationBytes)
        );
    }
}

#[test]
fn message_and_rule_times_string_work_is_charged_before_checks() {
    let message = "x".repeat(1024 * 1024);
    let rule = super::super::ZodStringRule::Step(super::super::ZodStringStep::Min(2));
    let schema = ZodJsonSchema::Array {
        element: Box::new(ZodJsonSchema::String(vec![ZodStringCheck {
            rule,
            message: Some(&message),
        }])),
        checks: vec![],
    };
    assert_eq!(
        zod_json(Some(&json!(vec!["x"; 100])), &schema),
        Err(ZodJsonUnsupported::EvaluationBytes)
    );
    let schema = ZodJsonSchema::String(vec![
        ZodStringCheck {
            rule,
            message: Some(&message)
        };
        17
    ]);
    assert_eq!(
        zod_json(None, &schema),
        Err(ZodJsonUnsupported::SchemaBytes)
    );
    let schema = ZodJsonSchema::String(vec![
        ZodStringCheck {
            rule,
            message: None
        };
        1000
    ]);
    let input = json!("x".repeat(1024 * 1024));
    let mut budget = Budget::new();
    assert!(matches!(
        evaluate(Some(&input), &schema, &mut budget),
        Err(ZodJsonUnsupported::EvaluationBytes)
    ));
    // Application and rules are reserved before even computing byte metrics.
    assert_eq!(budget.nodes, MAX_NODES - 1001);
}

#[test]
fn unknown_normalizes_large_integer_values_without_mutating_input() {
    let input: Value =
        serde_json::from_str("{\"n\":[9007199254740993,18446744073709551615]}").unwrap();
    let original = input.to_string();
    let ZodJsonOutcome::Valid(Some(output)) =
        zod_json(Some(&input), &ZodJsonSchema::Unknown).unwrap()
    else {
        panic!("valid normalized output");
    };
    assert_eq!(
        output["n"][0].as_f64().unwrap().to_bits(),
        9_007_199_254_740_992.0f64.to_bits()
    );
    assert_eq!(
        output["n"][1].as_f64().unwrap().to_bits(),
        18_446_744_073_709_551_616.0f64.to_bits()
    );
    assert_eq!(input.to_string(), original);
}

#[test]
fn arbitrary_precision_nonfinite_values_are_rejected_in_every_input_position() {
    // The normal serde profile rejects this literal at construction. The second
    // test command enables arbitrary_precision, as IAM dependency unification does.
    let Ok(value) = serde_json::from_str::<Value>("1e309") else {
        return;
    };
    for input in [
        value.clone(),
        json!([value.clone()]),
        json!({"extra":value.clone()}),
    ] {
        assert_eq!(
            zod_json(Some(&input), &ZodJsonSchema::Unknown),
            Err(ZodJsonUnsupported::NonFiniteNumber)
        );
        assert_eq!(
            zod_json(Some(&input), &ZodJsonSchema::Object(vec![])),
            Err(ZodJsonUnsupported::NonFiniteNumber)
        );
    }
    let schema = ZodJsonSchema::Default {
        schema: Box::new(ZodJsonSchema::Unknown),
        value: &value,
    };
    assert_eq!(
        zod_json(None, &schema),
        Err(ZodJsonUnsupported::NonFiniteNumber)
    );
}

#[test]
fn ordered_unions_keep_first_valid_or_first_dirty_and_empty_semantics() {
    let dirty = ZodJsonSchema::String(vec![ZodStringCheck {
        rule: super::super::ZodStringRule::Step(super::super::ZodStringStep::Min(10)),
        message: None,
    }]);
    let trim = ZodJsonSchema::String(vec![ZodStringCheck {
        rule: super::super::ZodStringRule::Step(super::super::ZodStringStep::Trim),
        message: None,
    }]);
    assert_eq!(
        zod_json(
            Some(&json!(" x ")),
            &ZodJsonSchema::Union(vec![dirty.clone(), trim])
        )
        .unwrap(),
        ZodJsonOutcome::Valid(Some(json!("x")))
    );
    let first = zod_json(Some(&json!("x")), &dirty).unwrap();
    assert_eq!(
        zod_json(
            Some(&json!("x")),
            &ZodJsonSchema::Union(vec![dirty, ZodJsonSchema::Boolean])
        )
        .unwrap(),
        first
    );
    assert_eq!(
        zod_json(Some(&json!(null)), &ZodJsonSchema::Union(vec![])).unwrap(),
        ZodJsonOutcome::Invalid(vec![])
    );
    assert_eq!(
        zod_json(
            None,
            &ZodJsonSchema::Union(vec![
                ZodJsonSchema::Optional(Box::new(ZodJsonSchema::String(vec![]))),
                ZodJsonSchema::Unknown
            ])
        )
        .unwrap(),
        ZodJsonOutcome::Valid(None)
    );
}

#[test]
fn union_adapter_duplicates_outer_paths_and_flattens_only_one_level() {
    let inner = ZodJsonSchema::Union(vec![
        ZodJsonSchema::String(vec![]),
        ZodJsonSchema::Object(vec![]),
    ]);
    let schema = ZodJsonSchema::Object(vec![ZodJsonField {
        name: "outer",
        schema: ZodJsonSchema::Union(vec![
            inner,
            ZodJsonSchema::Array {
                element: Box::new(ZodJsonSchema::Unknown),
                checks: vec![],
            },
        ]),
    }]);
    let ZodJsonOutcome::Invalid(errors) = zod_json(Some(&json!({"outer":false})), &schema).unwrap()
    else {
        panic!("expected invalid union")
    };
    assert_eq!(
        errors,
        vec![
            ParseError {
                path: vec![
                    "Union Schema Variant 0".into(),
                    "outer".into(),
                    "outer".into()
                ],
                message: "Invalid input".into()
            },
            ParseError {
                path: vec![
                    "Union Schema Variant 1".into(),
                    "outer".into(),
                    "outer".into()
                ],
                message: "Expected array, received boolean".into()
            },
        ]
    );
}

#[test]
fn union_branch_work_and_duplicated_diagnostic_paths_are_bounded() {
    let schema = ZodJsonSchema::Array {
        element: Box::new(ZodJsonSchema::Union(vec![
            ZodJsonSchema::String(vec![]);
            100
        ])),
        checks: vec![],
    };
    assert_eq!(
        zod_json(Some(&json!(vec![false; 1000])), &schema),
        Err(ZodJsonUnsupported::EvaluationNodes)
    );
    let key = "k".repeat(1024 * 1024);
    let schema = ZodJsonSchema::Object(vec![ZodJsonField {
        name: &key,
        schema: ZodJsonSchema::Union(vec![ZodJsonSchema::String(vec![]); 15]),
    }]);
    let input = Value::Object(Map::from_iter([(key.clone(), json!(false))]));
    assert_eq!(
        zod_json(Some(&input), &schema),
        Err(ZodJsonUnsupported::EvaluationBytes)
    );
    let schema = ZodJsonSchema::Union(vec![ZodJsonSchema::Unknown; MAX_NODES]);
    assert_eq!(
        zod_json(None, &schema),
        Err(ZodJsonUnsupported::SchemaNodes)
    );
}

#[test]
fn trusted_diagnostic_path_precedes_union_formatting_without_changing_output() {
    let schema = ZodJsonSchema::Union(vec![
        ZodJsonSchema::String(vec![]),
        ZodJsonSchema::Object(vec![]),
    ]);
    let input = json!(false);
    assert_eq!(
        zod_json_at_path(Some(&input), &schema, &[]),
        zod_json(Some(&input), &schema)
    );
    for path in [vec!["body"], vec!["request", "body"]] {
        let ZodJsonOutcome::Invalid(errors) =
            zod_json_at_path(Some(&input), &schema, &path).unwrap()
        else {
            panic!("expected invalid")
        };
        let expected = std::iter::once("Union Schema Variant 0")
            .chain(path.iter().copied())
            .chain(path.iter().copied())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(errors[0].path, expected);
        assert_eq!(
            zod_json_at_path(Some(&json!("valid")), &schema, &path),
            Ok(ZodJsonOutcome::Valid(Some(json!("valid"))))
        );
    }
    assert_eq!(input, json!(false));
}

#[test]
fn oversized_diagnostic_path_is_rejected_before_schema_or_input_evaluation() {
    let bad_schema = ZodJsonSchema::Object(vec![ZodJsonField {
        name: "__proto__",
        schema: ZodJsonSchema::Unknown,
    }]);
    assert_eq!(
        zod_json_at_path(None, &bad_schema, &["x"; MAX_DEPTH + 1]),
        Err(ZodJsonUnsupported::DiagnosticPath)
    );
    let huge = "p".repeat(MAX_ADMISSION_BYTES + 1);
    assert_eq!(
        zod_json_at_path(None, &bad_schema, &[&huge]),
        Err(ZodJsonUnsupported::DiagnosticPath)
    );
    let path = "p".repeat(1024 * 1024);
    let schema = ZodJsonSchema::Union(vec![ZodJsonSchema::String(vec![]); 16]);
    assert_eq!(
        zod_json_at_path(Some(&json!(false)), &schema, &[&path]),
        Err(ZodJsonUnsupported::EvaluationBytes)
    );
}

#[test]
fn diagnostic_path_shares_schema_default_admission_even_without_input() {
    let default = json!("d".repeat(9 * 1024 * 1024));
    let schema = ZodJsonSchema::Default {
        schema: Box::new(ZodJsonSchema::Unknown),
        value: &default,
    };
    let path = "p".repeat(8 * 1024 * 1024);
    assert_eq!(
        zod_json_at_path(None, &schema, &[&path]),
        Err(ZodJsonUnsupported::SchemaBytes)
    );
}

#[test]
fn literals_are_strict_preserve_input_zero_sign_and_abort_on_mismatch() {
    for expected in [-0.0, 0.0] {
        for input in [-0.0_f64, 0.0] {
            let raw = json!(input);
            let ZodJsonOutcome::Valid(Some(value)) = zod_json(
                Some(&raw),
                &ZodJsonSchema::Literal(ZodJsonLiteral::Number(expected)),
            )
            .unwrap() else {
                panic!("expected equal signed zero")
            };
            assert_eq!(value.as_f64().unwrap().to_bits(), input.to_bits());
        }
    }
    let schema = ZodJsonSchema::Literal(ZodJsonLiteral::Number(1.0));
    let invalid = zod_json(Some(&json!("1")), &schema).unwrap();
    assert_eq!(
        invalid,
        ZodJsonOutcome::Invalid(vec![ParseError {
            path: vec![],
            message: "Invalid literal value, expected 1".into()
        }])
    );
    assert!(
        evaluate(Some(&json!("1")), &schema, &mut Budget::new())
            .unwrap()
            .status
            == Status::Aborted
    );
    let raw: Value = serde_json::from_str("9007199254740993").unwrap();
    let before = raw.to_string();
    let ZodJsonOutcome::Valid(Some(value)) = zod_json(
        Some(&raw),
        &ZodJsonSchema::Literal(ZodJsonLiteral::Number(9_007_199_254_740_992.0)),
    )
    .unwrap() else {
        panic!("expected JS rounding")
    };
    assert_eq!(value.as_f64(), Some(9_007_199_254_740_992.0));
    assert_eq!(raw.to_string(), before);
}

#[test]
fn literal_diagnostics_follow_json_stringify_and_nonfinite_expected_values() {
    for expected in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            zod_json(
                None,
                &ZodJsonSchema::Literal(ZodJsonLiteral::Number(expected))
            ),
            Ok(ZodJsonOutcome::Invalid(vec![ParseError {
                path: vec![],
                message: "Invalid literal value, expected null".into()
            }]))
        );
    }
    let expected = "a\"\\\n\0\u{2028}😀";
    let literal = ZodJsonSchema::Literal(ZodJsonLiteral::String(expected));
    let ZodJsonOutcome::Invalid(errors) = zod_json(None, &literal).unwrap() else {
        panic!("expected literal failure")
    };
    assert_eq!(
        errors[0].message,
        format!(
            "Invalid literal value, expected {}",
            serde_json::to_string(expected).unwrap()
        )
    );
    assert_eq!(
        zod_json(Some(&json!(expected)), &literal),
        Ok(ZodJsonOutcome::Valid(Some(json!(expected))))
    );
}

#[test]
fn literal_escaped_diagnostics_and_repeated_schema_applications_are_bounded() {
    let expected = "\0".repeat(3 * 1024 * 1024);
    assert_eq!(
        zod_json(
            None,
            &ZodJsonSchema::Literal(ZodJsonLiteral::String(&expected))
        ),
        Err(ZodJsonUnsupported::EvaluationBytes)
    );
    let expected = "x".repeat(1024 * 1024);
    let schema = ZodJsonSchema::Array {
        element: Box::new(ZodJsonSchema::Literal(ZodJsonLiteral::String(&expected))),
        checks: vec![],
    };
    assert_eq!(
        zod_json(Some(&json!([false, false, false])), &schema),
        Err(ZodJsonUnsupported::EvaluationBytes)
    );
}

#[test]
fn repeated_scalar_metric_scans_charge_zero_byte_input_descendants() {
    let input = Value::Array(vec![Value::Object(Map::new()); 40_000]);
    let schema = ZodJsonSchema::Union(vec![ZodJsonSchema::String(vec![]); 3]);
    assert_eq!(
        zod_json(Some(&input), &schema),
        Err(ZodJsonUnsupported::EvaluationNodes)
    );
    let input = Value::Object(
        (0..20_000)
            .map(|index| (index.to_string(), Value::Array(vec![Value::Null])))
            .collect(),
    );
    let schema = ZodJsonSchema::Union(vec![
        ZodJsonSchema::Literal(ZodJsonLiteral::Boolean(true));
        3
    ]);
    assert_eq!(
        zod_json(Some(&input), &schema),
        Err(ZodJsonUnsupported::EvaluationNodes)
    );
}

#[test]
fn checked_array_errors_preserve_element_order_atomicity_and_limits() {
    use crate::zod_array::{ZodArrayCheck, ZodArrayRule};
    let check = |rule, message| ZodArrayCheck { rule, message };
    let schema = ZodJsonSchema::CheckedArray {
        element: Box::new(ZodJsonSchema::String(vec![ZodStringCheck {
            rule: super::super::ZodStringRule::Step(super::super::ZodStringStep::Trim),
            message: None,
        }])),
        checks: vec![
            check(
                ZodArrayRule::Max(super::super::ZodFloatBound::new(0.5)),
                Some("max"),
            ),
            check(
                ZodArrayRule::Min(super::super::ZodFloatBound::new(3.0)),
                Some("min"),
            ),
            check(
                ZodArrayRule::Length(super::super::ZodFloatBound::new(1.0)),
                Some(""),
            ),
        ],
    };
    let input = json!([" a ", false]);
    let original = input.clone();
    let ZodJsonOutcome::Invalid(errors) = zod_json(Some(&input), &schema).unwrap() else {
        panic!("invalid")
    };
    assert_eq!(
        errors
            .iter()
            .map(|e| e.message.as_str())
            .collect::<Vec<_>>(),
        ["", "min", "max", "Expected string, received boolean"]
    );
    assert_eq!(errors[3].path, ["1"]);
    assert_eq!(input, original);
    assert_eq!(
        zod_json(
            Some(&input),
            &ZodJsonSchema::Union(vec![schema, ZodJsonSchema::Unknown])
        ),
        Ok(ZodJsonOutcome::Valid(Some(original)))
    );

    let message = "x".repeat(1024 * 1024);
    let nested = ZodJsonSchema::CheckedArray {
        element: Box::new(ZodJsonSchema::Unknown),
        checks: vec![check(ZodArrayRule::Nonempty, Some(&message))],
    };
    let schema = ZodJsonSchema::Array {
        element: Box::new(nested),
        checks: vec![],
    };
    let input = json!(vec![Vec::<Value>::new(); 40]);
    assert_eq!(
        zod_json(Some(&input), &schema),
        Err(ZodJsonUnsupported::EvaluationBytes)
    );
    let huge = "x".repeat(MAX_ADMISSION_BYTES + 1);
    let schema = ZodJsonSchema::CheckedArray {
        element: Box::new(ZodJsonSchema::Unknown),
        checks: vec![check(ZodArrayRule::Nonempty, Some(&huge))],
    };
    assert_eq!(
        zod_json(None, &schema),
        Err(ZodJsonUnsupported::SchemaBytes)
    );
}

#[test]
fn checked_array_repeated_rule_work_is_charged_before_application() {
    use crate::zod_array::{ZodArrayCheck, ZodArrayRule};
    let schema = ZodJsonSchema::Array {
        checks: vec![],
        element: Box::new(ZodJsonSchema::CheckedArray {
            element: Box::new(ZodJsonSchema::Unknown),
            checks: vec![
                ZodArrayCheck {
                    rule: ZodArrayRule::Nonempty,
                    message: None
                };
                1000
            ],
        }),
    };
    assert_eq!(
        zod_json(Some(&json!(vec![vec![0]; 200])), &schema),
        Err(ZodJsonUnsupported::EvaluationNodes)
    );
}
