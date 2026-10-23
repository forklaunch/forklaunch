//! Callback-free structural discriminator registration and selected-only dispatch.
use super::enum_leaf::{Atom, AtomKey, native_values, tree_work};
use super::{
    Budget, Issue, Order, Origin, ParseError, Parsed, Status, ZodJsonLiteral, ZodJsonSchema,
    ZodJsonUnsupported,
};
use serde_json::Value;
use std::collections::BTreeSet;

fn push<'a>(
    tags: &mut Vec<Atom<'a>>,
    tag: Atom<'a>,
    budget: &mut Budget,
) -> Result<(), ZodJsonUnsupported> {
    budget.charge(1, 64)?;
    tags.push(tag);
    Ok(())
}

fn tags<'a>(
    schema: &'a ZodJsonSchema<'a>,
    output: &mut Vec<Atom<'a>>,
    budget: &mut Budget,
) -> Result<(), ZodJsonUnsupported> {
    budget.charge(1, 0)?;
    match schema {
        ZodJsonSchema::Literal(value) => push(
            output,
            match value {
                ZodJsonLiteral::String(value) => Atom::String(value),
                ZodJsonLiteral::Number(value) => Atom::Number(*value),
                ZodJsonLiteral::Boolean(value) => Atom::Boolean(*value),
                ZodJsonLiteral::Null => Atom::Null,
                ZodJsonLiteral::Undefined => Atom::Missing,
            },
            budget,
        )?,
        ZodJsonSchema::Enum(values) => {
            for value in values {
                push(output, Atom::String(value), budget)?;
            }
        }
        ZodJsonSchema::NativeEnum(entries) => {
            for value in native_values(entries, false, budget)? {
                push(output, value, budget)?;
            }
        }
        ZodJsonSchema::Null => push(output, Atom::Null, budget)?,
        ZodJsonSchema::Undefined => push(output, Atom::Missing, budget)?,
        ZodJsonSchema::Optional(inner) => {
            push(output, Atom::Missing, budget)?;
            tags(inner, output, budget)?;
        }
        ZodJsonSchema::Nullable(inner) => {
            push(output, Atom::Null, budget)?;
            tags(inner, output, budget)?;
        }
        ZodJsonSchema::Default { schema, .. } => tags(schema, output, budget)?,
        // An outer optional/nullable can still supply its one concrete tag.
        // In particular ordinary Union is not a discriminator enum in Zod.
        _ => {}
    }
    Ok(())
}

pub(super) fn registry<'a>(
    discriminator: &str,
    options: &'a [ZodJsonSchema<'a>],
    budget: &mut Budget,
) -> Result<Vec<(Atom<'a>, usize)>, ZodJsonUnsupported> {
    let mut registered = Vec::new();
    let mut seen: BTreeSet<AtomKey<'a>> = BTreeSet::new();
    for (index, option) in options.iter().enumerate() {
        budget.charge(1, 0)?;
        let (ZodJsonSchema::Object(fields)
        | ZodJsonSchema::StrictObject(fields)
        | ZodJsonSchema::ObjectWithPolicy { fields, .. }) = option
        else {
            return Err(ZodJsonUnsupported::DiscriminatorSchema);
        };
        let mut field = None;
        for candidate in fields {
            budget.charge(1, candidate.name.len().saturating_add(discriminator.len()))?;
            if candidate.name == discriminator {
                field = Some(&candidate.schema);
                break;
            }
        }
        let mut values = Vec::new();
        if let Some(field) = field {
            tags(field, &mut values, budget)?;
        }
        if values.is_empty() {
            return Err(ZodJsonUnsupported::DiscriminatorSchema);
        }
        for value in values {
            let work = tree_work(seen.len().saturating_add(1));
            budget.charge(work, value.bytes().saturating_mul(work).saturating_add(128))?;
            if !seen.insert(value.key()) {
                return Err(ZodJsonUnsupported::DuplicateDiscriminator);
            }
            registered.push((value, index));
        }
    }
    Ok(registered)
}

pub(super) fn evaluate(
    input: Option<&Value>,
    discriminator: &str,
    options: &[ZodJsonSchema<'_>],
    order: Option<&Order>,
    budget: &mut Budget,
) -> Result<Parsed, ZodJsonUnsupported> {
    let Some(Value::Object(values)) = input else {
        return super::object(input, &[], budget, None, false);
    };
    let registered = registry(discriminator, options, budget)?;
    let work = tree_work(values.len());
    budget.charge(work, discriminator.len().saturating_mul(work))?;
    let actual = Atom::from_input(values.get(discriminator)).map(Atom::key);
    for (value, index) in &registered {
        budget.charge(1, value.bytes())?;
        if actual.as_ref() == Some(&value.key()) {
            return super::evaluate_ordered(input, &options[*index], budget, order);
        }
    }
    budget.charge(
        registered.len(),
        registered
            .len()
            .saturating_mul(std::mem::size_of::<Atom<'_>>()),
    )?;
    let expected = super::enum_leaf::joined(
        &registered
            .into_iter()
            .map(|(value, _)| value)
            .collect::<Vec<_>>(),
        budget,
    )?;
    budget.charge(
        1,
        discriminator
            .len()
            .saturating_add(expected.len())
            .saturating_add(128),
    )?;
    Ok(Parsed {
        finite_output: true,
        merge_order_available: true,
        origin: Origin::default(),
        value: None,
        status: Status::Aborted,
        errors: vec![Issue::Leaf(ParseError {
            path: vec![discriminator.to_owned()],
            message: format!("Invalid discriminator value. Expected {expected}"),
        })],
    })
}

/// Registration is checked even for options that are not selected. This pass
/// runs only when schema admission encountered NativeEnum/DiscriminatedUnion.
pub(super) fn admit(
    schema: &ZodJsonSchema<'_>,
    budget: &mut Budget,
) -> Result<(), ZodJsonUnsupported> {
    budget.charge(1, 0)?;
    match schema {
        ZodJsonSchema::NativeEnum(entries) => {
            native_values(entries, false, budget)?;
        }
        ZodJsonSchema::DiscriminatedUnion {
            discriminator,
            options,
        } => {
            registry(discriminator, options, budget)?;
            for option in options {
                admit(option, budget)?;
            }
        }
        ZodJsonSchema::Union(options) => {
            for option in options {
                admit(option, budget)?;
            }
        }
        ZodJsonSchema::Intersection { left, right }
        | ZodJsonSchema::Record {
            key: left,
            value: right,
        } => {
            admit(left, budget)?;
            admit(right, budget)?;
        }
        ZodJsonSchema::Optional(inner)
        | ZodJsonSchema::Nullable(inner)
        | ZodJsonSchema::Default { schema: inner, .. }
        | ZodJsonSchema::Array { element: inner, .. }
        | ZodJsonSchema::CheckedArray { element: inner, .. } => admit(inner, budget)?,
        ZodJsonSchema::Tuple { items, rest } => {
            for item in items {
                admit(item, budget)?;
            }
            if let Some(rest) = rest {
                admit(rest, budget)?;
            }
        }
        ZodJsonSchema::Object(fields)
        | ZodJsonSchema::StrictObject(fields)
        | ZodJsonSchema::ObjectWithPolicy { fields, .. } => {
            for field in fields {
                admit(&field.schema, budget)?;
            }
            if let ZodJsonSchema::ObjectWithPolicy {
                catchall: Some(catchall),
                ..
            } = schema
            {
                admit(catchall, budget)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{
        ZodJsonEnumValue as EnumValue, ZodJsonField as Field, ZodJsonNativeEnumEntry as Entry,
        ZodJsonOutcome, zod_json,
    };
    use super::*;
    use serde_json::json;
    fn object(schema: ZodJsonSchema<'_>) -> ZodJsonSchema<'_> {
        ZodJsonSchema::Object(vec![Field {
            name: "kind",
            schema,
        }])
    }
    fn union(options: Vec<ZodJsonSchema<'_>>) -> ZodJsonSchema<'_> {
        ZodJsonSchema::DiscriminatedUnion {
            discriminator: "kind",
            options,
        }
    }
    #[test]
    fn native_reverse_names_dispatch_but_fail_selected_leaf() {
        let entries = vec![
            Entry {
                name: "0",
                value: EnumValue::String("A"),
            },
            Entry {
                name: "A",
                value: EnumValue::Number(0.0),
            },
        ];
        let schema = union(vec![object(ZodJsonSchema::NativeEnum(entries))]);
        let ZodJsonOutcome::Invalid(errors) =
            zod_json(Some(&json!({"kind":"A"})), &schema).unwrap()
        else {
            panic!("invalid")
        };
        assert_eq!(errors[0].path, ["kind"]);
        assert_eq!(
            errors[0].message,
            "Invalid enum value. Expected 0, received 'A'"
        );
        let ZodJsonOutcome::Valid(Some(value)) =
            zod_json(Some(&json!({"kind":0})), &schema).unwrap()
        else {
            panic!("valid")
        };
        assert_eq!(value["kind"].as_f64(), Some(0.0));
    }
    #[test]
    fn duplicate_tags_use_same_value_zero_even_before_input_validation() {
        for values in [[0.0, -0.0], [f64::NAN, f64::NAN]] {
            let schema = union(
                values
                    .into_iter()
                    .map(|value| object(ZodJsonSchema::Literal(ZodJsonLiteral::Number(value))))
                    .collect(),
            );
            assert_eq!(
                zod_json(Some(&Value::Null), &schema),
                Err(ZodJsonUnsupported::DuplicateDiscriminator)
            );
        }
        let schema = union(vec![object(ZodJsonSchema::Nullable(Box::new(
            ZodJsonSchema::Null,
        )))]);
        assert_eq!(
            zod_json(None, &schema),
            Err(ZodJsonUnsupported::DuplicateDiscriminator)
        );
    }
    #[test]
    fn optional_unsupported_inner_still_registers_absence() {
        let schema = union(vec![object(ZodJsonSchema::Optional(Box::new(
            ZodJsonSchema::String(vec![]),
        )))]);
        assert_eq!(
            zod_json(Some(&json!({})), &schema),
            Ok(ZodJsonOutcome::Valid(Some(json!({}))))
        );
        assert!(matches!(
            zod_json(Some(&json!({"kind":"a"})), &schema),
            Ok(ZodJsonOutcome::Invalid(_))
        ));
    }
    #[test]
    fn default_does_not_invent_absence_dispatch() {
        let fallback = json!("a");
        let schema = union(vec![object(ZodJsonSchema::Default {
            schema: Box::new(ZodJsonSchema::Literal(ZodJsonLiteral::String("a"))),
            value: &fallback,
        })]);
        let ZodJsonOutcome::Invalid(errors) = zod_json(Some(&json!({})), &schema).unwrap() else {
            panic!("invalid")
        };
        assert_eq!(
            errors[0].message,
            "Invalid discriminator value. Expected 'a'"
        );
    }
    #[test]
    fn duplicate_declared_fields_are_rejected_before_dispatch() {
        let schema = union(vec![ZodJsonSchema::Object(vec![
            Field {
                name: "kind",
                schema: ZodJsonSchema::Literal(ZodJsonLiteral::String("a")),
            },
            Field {
                name: "kind",
                schema: ZodJsonSchema::Literal(ZodJsonLiteral::String("b")),
            },
        ])]);
        assert_eq!(
            zod_json(Some(&json!({"kind":"a"})), &schema),
            Err(ZodJsonUnsupported::DuplicateField)
        );
    }
    #[test]
    fn unselected_option_does_not_spend_scalar_evaluation_budget() {
        use crate::typescript_coerce::{ZodStringCheck, ZodStringRule, ZodStringStep};
        let expensive = ZodJsonSchema::Object(vec![
            Field {
                name: "kind",
                schema: ZodJsonSchema::Literal(ZodJsonLiteral::String("b")),
            },
            Field {
                name: "value",
                schema: ZodJsonSchema::String(vec![
                    ZodStringCheck {
                        rule: ZodStringRule::Step(ZodStringStep::Trim),
                        message: None
                    };
                    1000
                ]),
            },
        ]);
        let options = vec![
            object(ZodJsonSchema::Literal(ZodJsonLiteral::String("a"))),
            expensive,
        ];
        let mut budget = Budget::new();
        budget.nodes = 500;
        let selected = evaluate(
            Some(&json!({"kind":"a","value":"x"})),
            "kind",
            &options,
            None,
            &mut budget,
        )
        .unwrap();
        assert!(matches!(selected.status, Status::Valid));
        let mut budget = Budget::new();
        budget.nodes = 500;
        assert!(matches!(
            evaluate(
                Some(&json!({"kind":"b","value":"x"})),
                "kind",
                &options,
                None,
                &mut budget
            ),
            Err(ZodJsonUnsupported::EvaluationNodes)
        ));
    }
    #[test]
    fn registration_and_diagnostic_work_is_bounded() {
        let options = vec![object(ZodJsonSchema::Enum(vec!["x"; 100_001]))];
        assert_eq!(
            zod_json(None, &union(options)),
            Err(ZodJsonUnsupported::SchemaNodes)
        );
        let long = "x".repeat(1024 * 1024);
        let schema = union(vec![object(ZodJsonSchema::Enum(vec![&long]))]);
        let mut budget = Budget::new();
        budget.nodes = 0;
        assert!(matches!(
            admit(&schema, &mut budget),
            Err(ZodJsonUnsupported::EvaluationNodes)
        ));
    }
}
