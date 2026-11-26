use super::{
    Budget, Order, Origin, ParseError, Parsed, Status, Value, ZodJsonSchema, ZodJsonUnsupported,
    evaluate_ordered, zod_array_shape,
};

pub(super) fn evaluate(
    input: Option<&Value>,
    items: &[ZodJsonSchema<'_>],
    rest: Option<&ZodJsonSchema<'_>>,
    budget: &mut Budget,
    order: Option<&Order>,
) -> Result<Parsed, ZodJsonUnsupported> {
    let values = match zod_array_shape(input, &[]) {
        Ok(values) => values,
        Err(errors) => return Ok(Parsed::invalid(errors, Status::Aborted)),
    };
    let length_error = |comparison| ParseError {
        path: Vec::new(),
        message: format!("Array must contain {comparison} {} element(s)", items.len()),
    };
    if values.len() < items.len() {
        return Ok(Parsed::invalid(
            vec![length_error("at least")],
            Status::Aborted,
        ));
    }
    let overlong = rest.is_none() && values.len() > items.len();
    let mut parsed = Parsed::invalid(
        if overlong {
            vec![length_error("at most")]
        } else {
            Vec::new()
        },
        if overlong {
            Status::Dirty
        } else {
            Status::Valid
        },
    );
    let count = if rest.is_some() {
        values.len()
    } else {
        items.len()
    };
    budget.charge(count, count.saturating_mul(std::mem::size_of::<Value>()))?;
    let origin_count = if budget.track_origins { count } else { 0 };
    budget.charge(
        0,
        origin_count.saturating_mul(std::mem::size_of::<Origin>()),
    )?;
    let mut origins = Vec::with_capacity(origin_count);
    let mut output = Vec::with_capacity(count);
    for (index, value) in values.iter().take(count).enumerate() {
        let schema = items.get(index).or(rest).expect("admitted tuple slot");
        let mut child = evaluate_ordered(
            Some(value),
            schema,
            budget,
            order.and_then(|order| order.index(index)),
        )?;
        parsed.absorb(&mut child, &index.to_string(), budget)?;
        if let Some(value) = child.value {
            output.push(value);
            if budget.track_origins {
                origins.push(child.origin);
            }
        }
    }
    if parsed.status == Status::Valid || (budget.track_origins && parsed.status == Status::Dirty) {
        parsed.value = Some(Value::Array(output));
        parsed.origin = Origin::Array {
            source: None,
            children: origins,
        };
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::super::{ZodJsonOutcome, zod_json};
    use super::*;
    use serde_json::json;

    #[test]
    fn optional_slots_still_require_length_and_surplus_is_dirty() {
        let schema = ZodJsonSchema::Tuple {
            items: vec![ZodJsonSchema::Optional(Box::new(ZodJsonSchema::String(
                vec![],
            )))],
            rest: None,
        };
        assert!(matches!(
            zod_json(Some(&json!([])), &schema),
            Ok(ZodJsonOutcome::Invalid(_))
        ));
        assert!(matches!(
            zod_json(Some(&json!(["a"])), &schema),
            Ok(ZodJsonOutcome::Valid(_))
        ));
        let outcome = zod_json(Some(&json!([false, "extra"])), &schema).unwrap();
        let ZodJsonOutcome::Invalid(errors) = outcome else {
            panic!("expected errors")
        };
        assert_eq!(errors[0].path, Vec::<String>::new());
        assert_eq!(errors[0].message, "Array must contain at most 1 element(s)");
        assert_eq!(errors[1].path, ["0"]);
    }

    #[test]
    fn short_tuple_never_evaluates_children() {
        let schema = ZodJsonSchema::Tuple {
            items: vec![ZodJsonSchema::Unknown],
            rest: None,
        };
        let mut budget = Budget::new();
        budget.nodes = 0;
        let parsed = evaluate(
            Some(&json!([])),
            &[ZodJsonSchema::Unknown],
            None,
            &mut budget,
            None,
        )
        .unwrap();
        assert!(parsed.status == Status::Aborted);
        assert!(matches!(
            zod_json(Some(&json!([])), &schema),
            Ok(ZodJsonOutcome::Invalid(_))
        ));
    }

    #[test]
    fn schema_and_repeated_rest_work_share_existing_limits() {
        let schema = ZodJsonSchema::Tuple {
            items: vec![ZodJsonSchema::Unknown; 100_001],
            rest: None,
        };
        assert_eq!(
            zod_json(Some(&json!([])), &schema),
            Err(ZodJsonUnsupported::SchemaNodes)
        );
        let mut budget = Budget::new();
        budget.nodes = 1;
        assert!(matches!(
            evaluate(
                Some(&json!([1, 2])),
                &[],
                Some(&ZodJsonSchema::Unknown),
                &mut budget,
                None
            ),
            Err(ZodJsonUnsupported::EvaluationNodes)
        ));
    }
    #[test]
    fn repeated_rest_defaults_are_byte_bounded_before_cloning() {
        let large = Value::String("x".repeat(1024 * 1024));
        let schema = ZodJsonSchema::Tuple {
            items: Vec::new(),
            rest: Some(Box::new(ZodJsonSchema::Object(vec![
                super::super::ZodJsonField {
                    name: "defaulted",
                    schema: ZodJsonSchema::Default {
                        schema: Box::new(ZodJsonSchema::Unknown),
                        value: &large,
                    },
                },
            ]))),
        };
        let input = Value::Array(vec![json!({}); 100]);
        assert_eq!(
            zod_json(Some(&input), &schema),
            Err(ZodJsonUnsupported::EvaluationBytes)
        );
    }
}
