//! Finite object policies; outputs are semantic JSON, not serialized own-key order.
use super::{
    Budget, Issue, Order, Origin, ParseError, Parsed, Status, ZodJsonField, ZodJsonSchema,
    ZodJsonUnknownKeys, ZodJsonUnsupported, evaluate_ordered, index_key,
};
use serde_json::{Map, Value};
#[derive(Default)]
struct ObjectOutput {
    values: Map<String, Value>,
    origins: std::collections::BTreeMap<String, Origin>,
}
pub(super) fn evaluate(
    input: Option<&Value>,
    fields: &[ZodJsonField<'_>],
    remaining: &mut Budget,
    order: Option<&Order>,
    unknown: ZodJsonUnknownKeys,
    catchall: Option<&ZodJsonSchema<'_>>,
) -> Result<Parsed, ZodJsonUnsupported> {
    let Some(Value::Object(values)) = input else {
        return Ok(Parsed::invalid(
            vec![ParseError {
                path: Vec::new(),
                message: input.map_or_else(
                    || "Required".to_owned(),
                    |value| {
                        format!(
                            "Expected object, received {}",
                            match value {
                                Value::Null => "null",
                                Value::Bool(_) => "boolean",
                                Value::Number(_) => "number",
                                Value::String(_) => "string",
                                Value::Array(_) => "array",
                                Value::Object(_) => "object",
                            }
                        )
                    },
                ),
            }],
            Status::Aborted,
        ));
    };
    let mut ordered: Vec<_> = fields.iter().collect();
    ordered.sort_by_key(|field| index_key(field.name));
    let mut parsed = Parsed::valid(None);
    let mut output = ObjectOutput::default();
    for field in ordered {
        let mut child = evaluate_ordered(
            values.get(field.name),
            &field.schema,
            remaining,
            order.and_then(|o| o.field(field.name)),
        )?;
        parsed.absorb(&mut child, field.name, remaining)?;
        if let Some(value) = child.value {
            remaining.charge(0, field.name.len())?;
            if remaining.track_origins {
                remaining.charge(
                    1,
                    field
                        .name
                        .len()
                        .saturating_add(std::mem::size_of::<Origin>()),
                )?;
                output.origins.insert(field.name.to_owned(), child.origin);
            }
            output.values.insert(field.name.to_owned(), value);
        }
    }
    let catchall = catchall.filter(|schema| !matches!(schema, ZodJsonSchema::Never));
    if catchall.is_some() || unknown == ZodJsonUnknownKeys::Passthrough {
        extras(
            values,
            fields,
            remaining,
            order,
            catchall,
            &mut parsed,
            &mut output,
        )?;
    } else if unknown == ZodJsonUnknownKeys::Strict {
        super::reject_unknown(values, fields, order, &mut parsed, remaining)?;
    }
    if parsed.status == Status::Valid || (remaining.track_origins && parsed.status == Status::Dirty)
    {
        parsed.value = Some(Value::Object(output.values));
        parsed.origin = Origin::Object {
            source: None,
            children: output.origins,
        };
    }
    Ok(parsed)
}

fn extras(
    values: &Map<String, Value>,
    fields: &[ZodJsonField<'_>],
    remaining: &mut Budget,
    order: Option<&Order>,
    catchall: Option<&ZodJsonSchema<'_>>,
    parsed: &mut Parsed,
    output: &mut ObjectOutput,
) -> Result<(), ZodJsonUnsupported> {
    remaining.charge(values.len() + fields.len(), values.len().saturating_mul(16))?;
    let declared: std::collections::BTreeSet<_> = fields.iter().map(|field| field.name).collect();
    let ordered = order.and_then(Order::keys);
    let mut keys: Vec<_> =
        ordered.map_or_else(|| values.keys().collect(), |keys| keys.iter().collect());
    keys.sort_by_key(|key| index_key(key));
    let extra_start = parsed.errors.len();
    let mut unordered_groups = 0;
    for name in keys {
        if declared.contains(name.as_str()) {
            continue;
        }
        let mut child = evaluate_ordered(
            values.get(name),
            catchall.unwrap_or(&ZodJsonSchema::Unknown),
            remaining,
            order.and_then(|order| order.field(name)),
        )?;
        if ordered.is_none() && index_key(name).0 && !child.errors.is_empty() {
            unordered_groups += 1;
        }
        parsed.absorb(&mut child, name, remaining)?;
        if name != "__proto__"
            && let Some(value) = child.value
        {
            remaining.charge(1, name.len())?;
            if remaining.track_origins {
                remaining.charge(1, name.len().saturating_add(std::mem::size_of::<Origin>()))?;
                output.origins.insert(name.clone(), child.origin);
            }
            output.values.insert(name.clone(), value);
        }
    }
    if unordered_groups > 1 {
        parsed.errors.truncate(extra_start);
        parsed.errors.push(Issue::UnorderedObject);
    }
    Ok(())
}
