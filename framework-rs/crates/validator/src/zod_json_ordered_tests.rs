use super::*;
use serde_json::json;

fn strict() -> ZodJsonSchema<'static> {
    ZodJsonSchema::StrictObject(vec![ZodJsonField {
        name: "known",
        schema: ZodJsonSchema::String(vec![]),
    }])
}
#[test]
fn own_keys_keep_duplicate_position_and_ecmascript_index_order() {
    let document = OrderedJsonDocument::parse(
        br#"{"z":1,"a":2,"10":3,"2":4,"01":5,"z":6,"4294967295":7,"4294967294":8}"#,
    )
    .unwrap();
    assert_eq!(
        document.order.keys().unwrap(),
        &["2", "10", "4294967294", "z", "a", "01", "4294967295"]
    );
    assert_eq!(document.value()["z"].as_f64(), Some(6.0));
    let ZodJsonOutcome::Invalid(errors) =
        zod_json_ordered_at_path(&document, &strict(), &["body"]).unwrap()
    else {
        panic!("expected errors")
    };
    assert_eq!(errors[0].path, ["body", "known"]);
    assert_eq!(errors[1].path, ["body"]);
    assert_eq!(
        errors[1].message,
        "Unrecognized key(s) in object: '2', '10', '4294967294', 'z', 'a', '01', '4294967295'"
    );
}
#[test]
fn order_is_requested_only_when_unknown_key_order_is_ambiguous() {
    assert!(matches!(
        zod_json(Some(&json!({"known":"yes"})), &strict()),
        Ok(ZodJsonOutcome::Valid(_))
    ));
    assert!(matches!(
        zod_json(Some(&json!({"z":1,"2":2,"10":3})), &strict()),
        Ok(ZodJsonOutcome::Invalid(_))
    ));
    assert_eq!(
        zod_json(Some(&json!({"z":1,"a":2})), &strict()),
        Err(ZodJsonUnsupported::ObjectOrderUnavailable)
    );
    let fallback = json!({"z":1,"a":2});
    let schema = ZodJsonSchema::Default {
        schema: Box::new(strict()),
        value: &fallback,
    };
    assert_eq!(
        zod_json(None, &schema),
        Err(ZodJsonUnsupported::ObjectOrderUnavailable)
    );
}
#[test]
fn numeric_protocol_does_not_capture_literal_object_keys() {
    // Without `arbitrary_precision`, the number marker is an ordinary key.
    {
        let raw = r#"{"$serde_json::private::Number":"1e309"}"#;
        let document = OrderedJsonDocument::parse(raw.as_bytes()).unwrap();
        assert!(document.value().is_object());
        assert_eq!(
            document.order.keys().unwrap().len(),
            document.value().as_object().unwrap().len()
        );
    }
    for (raw, expected) in [
        ("-0", -0.0_f64),
        ("9007199254740993", 9_007_199_254_740_992.0),
        ("18446744073709551615", 18_446_744_073_709_551_616.0),
    ] {
        let document = OrderedJsonDocument::parse(raw.as_bytes()).unwrap();
        assert_eq!(
            document.value().as_f64().unwrap().to_bits(),
            expected.to_bits()
        );
    }
    assert_eq!(
        OrderedJsonDocument::parse(b"1e309").unwrap_err(),
        OrderedJsonError::InvalidJson
    );
    assert_eq!(
        OrderedJsonDocument::parse(br#""\uD800""#).unwrap_err(),
        OrderedJsonError::InvalidJson
    );
}
#[test]
fn parsing_bounds_raw_depth_nodes_and_order_metadata_before_retention() {
    assert_eq!(
        OrderedJsonDocument::parse(&vec![b' '; MAX_ADMISSION_BYTES + 1]).unwrap_err(),
        OrderedJsonError::Unsupported(ZodJsonUnsupported::InputBytes)
    );
    let deep = format!("{}0{}", "[".repeat(65), "]".repeat(65));
    assert_eq!(
        OrderedJsonDocument::parse(deep.as_bytes()).unwrap_err(),
        OrderedJsonError::Unsupported(ZodJsonUnsupported::InputDepth)
    );
    let wide = format!("[{}0]", "0,".repeat(MAX_NODES));
    assert_eq!(
        OrderedJsonDocument::parse(wide.as_bytes()).unwrap_err(),
        OrderedJsonError::Unsupported(ZodJsonUnsupported::InputNodes)
    );
    // Raw is below 16 MiB, but value+local order key copies are not.
    let raw = format!("{{\"{}\":0}}", "k".repeat(6 * 1024 * 1024));
    assert_eq!(
        OrderedJsonDocument::parse(raw.as_bytes()).unwrap_err(),
        OrderedJsonError::Unsupported(ZodJsonUnsupported::InputBytes)
    );
    // Ancestor keys are retained only locally, never copied for every child path.
    let raw = format!(
        "{{\"{}\":[{}0]}}",
        "k".repeat(1024 * 1024),
        "0,".repeat(1000)
    );
    let document = OrderedJsonDocument::parse(raw.as_bytes()).unwrap();
    assert!(document.bytes < 4 * 1024 * 1024);
}
#[test]
fn strict_union_keeps_dirty_precedence_nested_order_and_atomic_output() {
    let document = OrderedJsonDocument::parse(br#"{"outer":{"known":"yes","z":1,"a":2}}"#).unwrap();
    let schema = ZodJsonSchema::Object(vec![ZodJsonField {
        name: "outer",
        schema: ZodJsonSchema::Union(vec![strict(), ZodJsonSchema::Boolean]),
    }]);
    let ZodJsonOutcome::Invalid(errors) = zod_json_ordered(&document, &schema).unwrap() else {
        panic!("expected dirty strict result")
    };
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].path, ["outer"]);
    assert_eq!(errors[0].message, "Unrecognized key(s) in object: 'z', 'a'");
    let schema = ZodJsonSchema::Union(vec![schema, ZodJsonSchema::Unknown]);
    assert!(matches!(
        zod_json_ordered(&document, &schema),
        Ok(ZodJsonOutcome::Valid(Some(_)))
    ));
    assert_eq!(document.value()["outer"]["z"].as_f64(), Some(1.0));
}

#[test]
fn unavailable_order_is_deferred_until_selected_diagnostics_are_observable() {
    let input = json!({"known":"yes","z":1,"a":2});
    let schema = ZodJsonSchema::Union(vec![strict(), ZodJsonSchema::Unknown]);
    assert!(matches!(
        zod_json(Some(&input), &schema),
        Ok(ZodJsonOutcome::Valid(Some(_)))
    ));
    let schema = ZodJsonSchema::Union(vec![strict(), ZodJsonSchema::Boolean]);
    assert_eq!(
        zod_json(Some(&input), &schema),
        Err(ZodJsonUnsupported::ObjectOrderUnavailable)
    );
    // A failed nested union's children are hidden by the actual one-level
    // formatter. Hidden unknown-key ordering must not turn that into Unsupported.
    let input = json!({"known":false,"z":1,"a":2});
    let nested = ZodJsonSchema::Union(vec![strict(), ZodJsonSchema::Boolean]);
    let schema = ZodJsonSchema::Union(vec![nested, ZodJsonSchema::String(vec![])]);
    let ZodJsonOutcome::Invalid(errors) = zod_json(Some(&input), &schema).unwrap() else {
        panic!("expected union failure")
    };
    assert_eq!(errors[0].message, "Invalid input");
}

#[test]
fn lexical_numeric_queue_skips_strings_keys_and_preserves_underflow_sign() {
    let raw = br#"{"3":["-0","\"123",-0,9007199254740993,{"\u0031":1.0}],"7":-1e-400,"8":1e-400,"3":[-0.0e+30]}"#;
    let document = OrderedJsonDocument::parse(raw).unwrap();
    assert_eq!(
        document.value()["3"][0].as_f64().unwrap().to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(
        document.value()["7"].as_f64().unwrap().to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(
        document.value()["8"].as_f64().unwrap().to_bits(),
        0.0_f64.to_bits()
    );
    assert_eq!(document.order.keys().unwrap(), &["3", "7", "8"]);
    for raw in [
        "[1 2]",
        "[1e]",
        "[-]",
        "[01]",
        "[+1]",
        "[1.]",
        "{\"1\":1 2}",
        "\"unterminated1",
        "true1",
        "[1] 2",
    ] {
        assert!(
            matches!(
                OrderedJsonDocument::parse(raw.as_bytes()),
                Err(OrderedJsonError::InvalidJson)
            ),
            "{raw}"
        );
    }
}
