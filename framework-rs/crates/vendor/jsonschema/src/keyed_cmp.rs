//! Key-order-independent JSON equality for `const`, `enum` and `uniqueItems`.
//!
//! `jsonschema_value::cmp` compares objects member by member in iteration order,
//! which is only correct for sorted maps. With `serde_json`'s `preserve_order`
//! (which forklaunch-core enables) maps keep insertion order, so two equal
//! objects can iterate differently. JSON Schema object equality ignores member
//! order, so objects are compared by key lookup here and uniqueness is checked
//! on key-sorted copies.
use std::borrow::Cow;

use serde_json::{Map, Value};

use crate::{Array, Json, Node};

/// Whether `value` is or contains an object, the only case iteration order matters.
pub(crate) fn contains_object(value: &Value) -> bool {
    match value {
        Value::Object(_) => true,
        Value::Array(items) => items.iter().any(contains_object),
        _ => false,
    }
}

/// JSON Schema equality: numbers compare mathematically, objects by member set.
pub(crate) fn equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .all(|(key, value)| right.get(key).is_some_and(|other| equal(value, other)))
        }
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len() && left.iter().zip(right).all(|(a, b)| equal(a, b))
        }
        _ => crate::cmp::equal(left, right),
    }
}

/// `instance == expected`, keeping the backend's fast path when no object is involved.
pub(crate) fn node_equals<F: Json>(instance: &F::Node<'_>, expected: &Value) -> bool {
    if contains_object(expected) {
        equal(&instance.to_value(), expected)
    } else {
        instance.equals_value(expected)
    }
}

fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut members: Vec<_> = map.iter().collect();
            members.sort_unstable_by(|a, b| a.0.cmp(b.0));
            Value::Object(
                members
                    .into_iter()
                    .map(|(key, value)| (key.clone(), sorted(value)))
                    .collect::<Map<_, _>>(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

/// `uniqueItems`: every element distinct under JSON Schema equality.
pub(crate) fn is_unique<'a, F: Json, A: Array<'a, F>>(array: &A) -> bool {
    let values: Vec<Cow<'a, Value>> = array.elements().map(|item| item.to_value()).collect();
    if values.iter().any(|value| contains_object(value)) {
        let sorted: Vec<Value> = values.iter().map(|value| sorted(value)).collect();
        crate::unique::is_unique(&sorted)
    } else {
        array.is_unique()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    fn object(pairs: &[(&str, Value)]) -> Value {
        Value::Object(
            pairs
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
        )
    }

    #[test]
    fn objects_compare_without_member_order() {
        let forward = object(&[("value", json!("")), ("b", json!(1))]);
        let reverse = object(&[("b", json!(1.0)), ("value", json!(""))]);
        assert!(super::equal(&forward, &reverse));
        assert!(super::equal(
            &json!([forward.clone()]),
            &json!([reverse.clone()])
        ));
        assert!(!super::equal(
            &forward,
            &object(&[("b", json!(1)), ("value", json!("x"))])
        ));
        assert!(!super::equal(&forward, &object(&[("b", json!(1))])));
    }

    #[test]
    fn validators_ignore_member_order() {
        let instance = object(&[("value", json!("")), ("b", json!(""))]);
        let reverse = object(&[("b", json!("")), ("value", json!(""))]);
        for schema in [
            json!({"const": reverse.clone()}),
            json!({"enum": [reverse.clone()]}),
            json!({"enum": [1, reverse.clone()]}),
            json!({"const": [reverse.clone()]}),
        ] {
            let target = if schema["const"].is_array() {
                json!([instance.clone()])
            } else {
                instance.clone()
            };
            assert!(crate::is_valid(&schema, &target), "{schema}");
        }
        let unique = json!({"uniqueItems": true});
        assert!(!crate::is_valid(
            &unique,
            &json!([instance.clone(), reverse.clone()])
        ));
        assert!(crate::is_valid(
            &unique,
            &json!([instance, {"b": "", "value": "x"}])
        ));
    }
}
