//! Private intersection merge and source-reference provenance.
use super::{Budget, ZodJsonUnsupported};
use serde_json::Value;
use std::collections::BTreeMap;

/// Tokens are compared only, never dereferenced. Borrowed input/default
/// containers remain alive for the entire evaluation. Temporary record keys
/// are scalars and never receive a token.
#[derive(Default)]
pub(super) enum Origin {
    #[default]
    Scalar,
    Array {
        source: Option<usize>,
        children: Vec<Self>,
    },
    Object {
        source: Option<usize>,
        children: BTreeMap<String, Self>,
    },
}

impl Origin {
    pub(super) fn borrowed(value: &Value, budget: &mut Budget) -> Result<Self, ZodJsonUnsupported> {
        budget.charge(1, 0)?;
        let source = Some(std::ptr::from_ref(value) as usize);
        match value {
            Value::Array(values) => {
                budget.charge(0, values.len().saturating_mul(std::mem::size_of::<Self>()))?;
                let mut children = Vec::with_capacity(values.len());
                for value in values {
                    children.push(Self::borrowed(value, budget)?);
                }
                Ok(Self::Array { source, children })
            }
            Value::Object(values) => {
                let mut children = BTreeMap::new();
                for (key, value) in values {
                    budget.charge(1, key.len().saturating_add(std::mem::size_of::<Self>()))?;
                    children.insert(key.clone(), Self::borrowed(value, budget)?);
                }
                Ok(Self::Object { source, children })
            }
            _ => Ok(Self::Scalar),
        }
    }

    fn source(&self) -> Option<usize> {
        match self {
            Self::Array { source, .. } | Self::Object { source, .. } => *source,
            Self::Scalar => None,
        }
    }

    fn take_index(&mut self, index: usize) -> Self {
        if let Self::Array { children, .. } = self
            && let Some(origin) = children.get_mut(index)
        {
            return std::mem::take(origin);
        }
        Self::Scalar
    }

    fn take_key(&mut self, key: &str) -> Self {
        if let Self::Object { children, .. } = self {
            return children.remove(key).unwrap_or_default();
        }
        Self::Scalar
    }
}

/// `None` is a semantic merge conflict, distinct from a resource failure.
/// Owned branch outputs move into the result; no full output clone is needed.
pub(super) fn merge(
    left: Option<Value>,
    mut left_origin: Origin,
    right: Option<Value>,
    mut right_origin: Origin,
    budget: &mut Budget,
) -> Result<Option<(Option<Value>, Origin)>, ZodJsonUnsupported> {
    budget.charge(1, 0)?;
    if let Some(source) = left_origin.source()
        && right_origin.source() == Some(source)
    {
        return Ok(Some((left, left_origin)));
    }
    let (left, right) = match (left, right) {
        (None, None) => return Ok(Some((None, Origin::Scalar))),
        (Some(left), Some(right)) => (left, right),
        _ => return Ok(None),
    };
    match (left, right) {
        (Value::Array(left), Value::Array(right)) => {
            if left.len() != right.len() {
                return Ok(None);
            }
            budget.charge(
                left.len(),
                left.len().saturating_mul(
                    std::mem::size_of::<Value>().saturating_add(std::mem::size_of::<Origin>()),
                ),
            )?;
            let mut values = Vec::with_capacity(left.len());
            let mut children = Vec::with_capacity(left.len());
            for (index, (left, right)) in left.into_iter().zip(right).enumerate() {
                let Some((value, origin)) = merge(
                    Some(left),
                    left_origin.take_index(index),
                    Some(right),
                    right_origin.take_index(index),
                    budget,
                )?
                else {
                    return Ok(None);
                };
                let Some(value) = value else {
                    return Ok(None);
                };
                values.push(value);
                children.push(origin);
            }
            Ok(Some((
                Some(Value::Array(values)),
                Origin::Array {
                    source: None,
                    children,
                },
            )))
        }
        (Value::Object(mut left), Value::Object(mut right)) => {
            left.remove("__proto__");
            right.remove("__proto__");
            let mut children = BTreeMap::new();
            for (key, value) in &mut left {
                budget.charge(1, key.len().saturating_add(std::mem::size_of::<Origin>()))?;
                let origin = if let Some(right) = right.remove(key) {
                    let Some((merged, origin)) = merge(
                        Some(std::mem::take(value)),
                        left_origin.take_key(key),
                        Some(right),
                        right_origin.take_key(key),
                        budget,
                    )?
                    else {
                        return Ok(None);
                    };
                    let Some(merged) = merged else {
                        return Ok(None);
                    };
                    *value = merged;
                    origin
                } else {
                    left_origin.take_key(key)
                };
                children.insert(key.clone(), origin);
            }
            for (key, value) in right {
                budget.charge(1, key.len().saturating_add(std::mem::size_of::<Origin>()))?;
                children.insert(key.clone(), right_origin.take_key(&key));
                left.insert(key, value);
            }
            Ok(Some((
                Some(Value::Object(left)),
                Origin::Object {
                    source: None,
                    children,
                },
            )))
        }
        (Value::String(left), Value::String(right)) => {
            budget.charge(0, left.len().saturating_add(right.len()))?;
            Ok((left == right).then_some((Some(Value::String(left)), Origin::Scalar)))
        }
        (left, right) if left == right => Ok(Some((Some(left), Origin::Scalar))),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{ZodJsonField, ZodJsonOutcome, ZodJsonSchema, zod_json};
    use super::*;
    use crate::typescript_coerce::{ZodStringCheck, ZodStringRule, ZodStringStep};
    use serde_json::json;

    fn intersection<'a>(left: ZodJsonSchema<'a>, right: ZodJsonSchema<'a>) -> ZodJsonSchema<'a> {
        ZodJsonSchema::Intersection {
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    fn string(steps: &[ZodStringStep]) -> ZodJsonSchema<'static> {
        ZodJsonSchema::String(
            steps
                .iter()
                .map(|step| ZodStringCheck {
                    rule: ZodStringRule::Step(*step),
                    message: None,
                })
                .collect(),
        )
    }

    #[test]
    fn dirty_transform_merges_before_atomic_failure() {
        let schema = intersection(
            string(&[ZodStringStep::Min(4), ZodStringStep::Trim]),
            string(&[]),
        );
        let input = json!(" x ");
        let ZodJsonOutcome::Invalid(errors) = zod_json(Some(&input), &schema).unwrap() else {
            panic!("invalid");
        };
        assert_eq!(errors.len(), 2);
        assert_eq!(
            errors[0].message,
            "String must contain at least 4 character(s)"
        );
        assert_eq!(
            errors[1].message,
            "Intersection results could not be merged"
        );
        assert_eq!(input, json!(" x "));
    }

    #[test]
    fn shared_nested_unknown_retains_source_identity() {
        let input: Value =
            serde_json::from_str(r#"{"nested":{"__proto__":{"retained":true},"x":1}}"#).unwrap();
        let object = || {
            ZodJsonSchema::Object(vec![ZodJsonField {
                name: "nested",
                schema: ZodJsonSchema::Unknown,
            }])
        };
        assert_eq!(
            zod_json(Some(&input), &intersection(object(), object())).unwrap(),
            ZodJsonOutcome::Valid(Some(super::super::clone_json(&input).unwrap()))
        );
    }

    #[test]
    fn equal_but_distinct_defaults_do_not_share_identity() {
        let left: Value = serde_json::from_str(r#"{"__proto__":{"retained":true},"x":1}"#).unwrap();
        let right = left.clone();
        let default = |value| ZodJsonSchema::Default {
            schema: Box::new(ZodJsonSchema::Unknown),
            value,
        };
        assert_eq!(
            zod_json(None, &intersection(default(&left), default(&left))).unwrap(),
            ZodJsonOutcome::Valid(Some(super::super::clone_json(&left).unwrap()))
        );
        assert_eq!(
            zod_json(None, &intersection(default(&left), default(&right))).unwrap(),
            ZodJsonOutcome::Valid(Some(json!({"x":1.0})))
        );
    }

    #[test]
    fn aborted_branches_both_diagnose_without_merge_issue() {
        let schema = intersection(
            string(&[]),
            ZodJsonSchema::Array {
                element: Box::new(string(&[])),
                checks: vec![],
            },
        );
        let ZodJsonOutcome::Invalid(errors) = zod_json(Some(&json!(false)), &schema).unwrap()
        else {
            panic!("invalid");
        };
        assert_eq!(
            errors
                .iter()
                .map(|error| error.message.as_str())
                .collect::<Vec<_>>(),
            [
                "Expected string, received boolean",
                "Expected array, received boolean"
            ]
        );
    }

    #[test]
    fn dirty_record_order_is_deferred_until_selected() {
        let input = json!({" a": 1, "a ": 2});
        let record = || ZodJsonSchema::Record {
            key: Box::new(string(&[ZodStringStep::Min(4), ZodStringStep::Trim])),
            value: Box::new(ZodJsonSchema::Unknown),
        };
        let ambiguous = || intersection(record(), ZodJsonSchema::Unknown);
        assert_eq!(
            zod_json(Some(&input), &ambiguous()),
            Err(ZodJsonUnsupported::ObjectOrderUnavailable)
        );
        let schema = ZodJsonSchema::Union(vec![ambiguous(), ZodJsonSchema::Unknown]);
        let ZodJsonOutcome::Valid(Some(output)) = zod_json(Some(&input), &schema).unwrap() else {
            panic!("later branch wins");
        };
        assert_eq!(output[" a"].as_f64(), Some(1.0));
        assert_eq!(output["a "].as_f64(), Some(2.0));
    }

    #[test]
    fn scalar_equality_scan_consumes_shared_byte_budget() {
        let mut budget = Budget::new();
        let left = Value::String("x".repeat(1024 * 1024));
        for _ in 0..16 {
            assert!(
                merge(
                    Some(left.clone()),
                    Origin::default(),
                    Some(left.clone()),
                    Origin::default(),
                    &mut budget
                )
                .unwrap()
                .is_some()
            );
        }
        assert!(matches!(
            merge(
                Some(left.clone()),
                Origin::default(),
                Some(left),
                Origin::default(),
                &mut budget
            ),
            Err(ZodJsonUnsupported::EvaluationBytes)
        ));
    }

    #[test]
    fn merge_and_provenance_charge_before_allocation() {
        let input = json!([{}, {}, {}]);
        let mut budget = Budget::new();
        budget.nodes = 1;
        assert!(matches!(
            Origin::borrowed(&input, &mut budget),
            Err(ZodJsonUnsupported::EvaluationNodes)
        ));
        let mut budget = Budget::new();
        budget.nodes = 0;
        assert!(matches!(
            merge(
                Some(input.clone()),
                Origin::default(),
                Some(input),
                Origin::default(),
                &mut budget
            ),
            Err(ZodJsonUnsupported::EvaluationNodes)
        ));
    }
}
