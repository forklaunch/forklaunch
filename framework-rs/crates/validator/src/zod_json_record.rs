//! Own-key ordered records over the evaluator's existing schema grammar.
use super::{
    Budget, Issue, Order, Origin, Parsed, Status, ZodJsonSchema, ZodJsonUnsupported,
    evaluate_ordered, index_key,
};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub(super) fn evaluate(
    input: Option<&Value>,
    key_schema: &ZodJsonSchema<'_>,
    value_schema: &ZodJsonSchema<'_>,
    remaining: &mut Budget,
    order: Option<&Order>,
) -> Result<Parsed, ZodJsonUnsupported> {
    let Some(Value::Object(values)) = input else {
        return super::object(input, &[], remaining, None, false);
    };
    remaining.charge(values.len(), values.len().saturating_mul(16))?;
    let ordered = order.and_then(Order::keys);
    let mut keys: Vec<_> =
        ordered.map_or_else(|| values.keys().collect(), |keys| keys.iter().collect());
    keys.sort_by_key(|key| index_key(key));
    let mut parsed = Parsed::valid(None);
    let mut output = Map::new();
    let mut output_origins = BTreeMap::new();
    let mut origins = BTreeMap::<String, &str>::new();
    let mut ambiguous_collision = false;
    let mut unordered_error_groups = 0;
    for name in keys {
        remaining.charge(1, name.len())?;
        let key_input = Value::String(name.clone());
        let mut key = evaluate_ordered(Some(&key_input), key_schema, remaining, None)?;
        // Source validates the value even when its key already aborted.
        let mut value = evaluate_ordered(
            values.get(name),
            value_schema,
            remaining,
            order.and_then(|order| order.field(name)),
        )?;
        if ordered.is_none()
            && index_key(name).0
            && (!key.errors.is_empty() || !value.errors.is_empty())
        {
            unordered_error_groups += 1;
        }
        let mergeable = if remaining.track_origins {
            key.status != Status::Aborted && value.status != Status::Aborted
        } else {
            key.status == Status::Valid && value.status == Status::Valid
        };
        parsed.absorb(&mut key, name, remaining)?;
        parsed.absorb(&mut value, name, remaining)?;
        if mergeable {
            let name_out = property_key(key.value, remaining)?;
            if name_out == "__proto__" {
                continue;
            }
            remaining.charge(1, name_out.len().saturating_mul(2).saturating_add(32))?;
            if let Some(previous) = origins.insert(name_out.clone(), name)
                && ordered.is_none()
                && index_key(previous).0
                && index_key(name).0
            {
                ambiguous_collision = true;
            }
            if remaining.track_origins {
                remaining.charge(
                    1,
                    name_out.len().saturating_add(std::mem::size_of::<Origin>()),
                )?;
                output_origins.insert(name_out.clone(), value.origin);
            }
            let value = value.value.ok_or(ZodJsonUnsupported::RecordKeyOutput)?;
            output.insert(name_out, value);
        }
    }
    if parsed.status == Status::Valid || (remaining.track_origins && parsed.status == Status::Dirty)
    {
        if ambiguous_collision {
            if parsed.status == Status::Valid {
                return Err(ZodJsonUnsupported::ObjectOrderUnavailable);
            }
            parsed.merge_order_available = false;
        }
        parsed.value = Some(Value::Object(output));
        parsed.origin = Origin::Object {
            source: None,
            children: output_origins,
        };
    }
    if parsed.status != Status::Valid && unordered_error_groups > 1 {
        // The statuses are order-independent for these pure schemas. Keep the
        // result dirty/aborted so later valid unions can win, deferring missing
        // diagnostic order until the selected raw issues are actually formatted.
        parsed.errors = vec![Issue::UnorderedObject];
    }
    Ok(parsed)
}

fn property_key(
    value: Option<Value>,
    remaining: &mut Budget,
) -> Result<String, ZodJsonUnsupported> {
    match value {
        Some(Value::String(value)) => Ok(value),
        Some(Value::Bool(value)) => {
            remaining.charge(0, 5)?;
            Ok(value.to_string())
        }
        Some(Value::Number(value)) => {
            let number = value
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or(ZodJsonUnsupported::NonFiniteNumber)?;
            let mut buffer = ryu_js::Buffer::new();
            let value = buffer.format(number);
            remaining.charge(0, value.len())?;
            Ok(value.to_owned())
        }
        _ => Err(ZodJsonUnsupported::RecordKeyOutput),
    }
}
