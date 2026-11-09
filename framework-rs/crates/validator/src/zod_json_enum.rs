//! Raw Zod enum leaves, distinct from the public factory's literal-union lowering.
use super::{Budget, ParseError, Parsed, Status, ZodJsonUnsupported};
use serde_json::Value;
use std::collections::BTreeMap;

/// Own values of a TypeScript-style native enum. Input remains finite JSON;
/// nonfinite expected values participate in registration and diagnostics only.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZodJsonEnumValue<'a> {
    String(&'a str),
    Number(f64),
}

/// One unique own enum property, in insertion order. Integer-index keys are
/// evaluated in ECMAScript own-key order. No getters or inherited values run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZodJsonNativeEnumEntry<'a> {
    pub name: &'a str,
    pub value: ZodJsonEnumValue<'a>,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum Atom<'a> {
    Missing,
    Null,
    String(&'a str),
    Number(f64),
    Boolean(bool),
}
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum AtomKey<'a> {
    Missing,
    Null,
    String(&'a str),
    Number(u64),
    Boolean(bool),
}
impl<'a> Atom<'a> {
    pub(super) fn key(self) -> AtomKey<'a> {
        match self {
            Self::Missing => AtomKey::Missing,
            Self::Null => AtomKey::Null,
            Self::String(value) => AtomKey::String(value),
            Self::Number(value) => AtomKey::Number(if value.is_nan() {
                f64::NAN.to_bits()
            } else if value == 0.0 {
                0
            } else {
                value.to_bits()
            }),
            Self::Boolean(value) => AtomKey::Boolean(value),
        }
    }
    pub(super) fn bytes(self) -> usize {
        match self {
            Self::String(value) => value.len(),
            _ => 32,
        }
    }
    pub(super) fn from_input(input: Option<&'a Value>) -> Option<Self> {
        match input {
            None => Some(Self::Missing),
            Some(Value::Null) => Some(Self::Null),
            Some(Value::String(value)) => Some(Self::String(value)),
            Some(Value::Number(value)) => value.as_f64().map(Self::Number),
            Some(Value::Bool(value)) => Some(Self::Boolean(*value)),
            _ => None,
        }
    }
}
impl<'a> From<ZodJsonEnumValue<'a>> for Atom<'a> {
    fn from(value: ZodJsonEnumValue<'a>) -> Self {
        match value {
            ZodJsonEnumValue::String(value) => Self::String(value),
            ZodJsonEnumValue::Number(value) => Self::Number(value),
        }
    }
}

pub(super) fn tree_work(count: usize) -> usize {
    16usize.saturating_mul(count.max(1).ilog2() as usize + 1)
}

pub(super) fn native_values<'a>(
    entries: &'a [ZodJsonNativeEnumEntry<'a>],
    filtered: bool,
    budget: &mut Budget,
) -> Result<Vec<Atom<'a>>, ZodJsonUnsupported> {
    budget.charge(entries.len(), entries.len().saturating_mul(128))?;
    let work = tree_work(entries.len());
    let mut table = BTreeMap::new();
    let mut ordered = Vec::with_capacity(entries.len());
    for entry in entries {
        budget.charge(work, entry.name.len().saturating_mul(work))?;
        if table.insert(entry.name, entry.value).is_some() {
            return Err(ZodJsonUnsupported::DuplicateEnumKey);
        }
        ordered.push(entry);
    }
    ordered.sort_by_key(|entry| super::index_key(entry.name));
    let mut values = Vec::with_capacity(ordered.len());
    for entry in ordered {
        if filtered {
            // getValidEnumValues assigns to an ordinary {}. Its __proto__
            // setter ignores these scalar values; raw dispatch still sees it.
            if entry.name == "__proto__" {
                continue;
            }
            let mut number = ryu_js::Buffer::new();
            let key = match entry.value {
                ZodJsonEnumValue::String(value) => value,
                ZodJsonEnumValue::Number(value) => number.format(value),
            };
            budget.charge(work, key.len().saturating_mul(work))?;
            if matches!(table.get(key), Some(ZodJsonEnumValue::Number(_))) {
                continue;
            }
        }
        values.push(entry.value.into());
    }
    Ok(values)
}

pub(super) fn joined(
    values: &[Atom<'_>],
    budget: &mut Budget,
) -> Result<String, ZodJsonUnsupported> {
    let bytes = values.iter().fold(0usize, |sum, value| {
        sum.saturating_add(value.bytes()).saturating_add(5)
    });
    budget.charge(values.len(), bytes)?;
    let mut output = String::with_capacity(bytes);
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            output.push_str(" | ");
        }
        match value {
            Atom::Missing | Atom::Null => {}
            Atom::String(value) => {
                output.push('\'');
                output.push_str(value);
                output.push('\'');
            }
            Atom::Number(value) => output.push_str(ryu_js::Buffer::new().format(*value)),
            Atom::Boolean(value) => output.push_str(if *value { "true" } else { "false" }),
        }
    }
    Ok(output)
}

pub(super) fn evaluate(
    input: Option<&Value>,
    values: &[Atom<'_>],
    native: bool,
    budget: &mut Budget,
) -> Result<Parsed, ZodJsonUnsupported> {
    let correct_type = matches!(input, Some(Value::String(_)))
        || (native && matches!(input, Some(Value::Number(_))));
    if correct_type {
        let actual = Atom::from_input(input).expect("admitted scalar").key();
        for value in values {
            budget.charge(1, value.bytes())?;
            if value.key() == actual {
                return Ok(Parsed::valid(input.map(super::clone_json).transpose()?));
            }
        }
    }
    let expected = joined(values, budget)?;
    budget.charge(
        1,
        expected
            .len()
            .saturating_add(input.and_then(Value::as_str).map_or(64, str::len))
            .saturating_add(128),
    )?;
    let message = if input.is_none() {
        "Required".to_owned()
    } else if !correct_type {
        let kind = match input {
            Some(Value::Null) => "null",
            Some(Value::Bool(_)) => "boolean",
            Some(Value::Number(_)) => "number",
            Some(Value::Array(_)) => "array",
            Some(Value::Object(_)) => "object",
            _ => unreachable!(),
        };
        format!("Expected {expected}, received {kind}")
    } else {
        let mut buffer = ryu_js::Buffer::new();
        let received = match input {
            Some(Value::String(value)) => value.as_str(),
            Some(Value::Number(value)) => buffer.format(value.as_f64().expect("finite input")),
            _ => unreachable!(),
        };
        format!("Invalid enum value. Expected {expected}, received '{received}'")
    };
    Ok(Parsed::invalid(
        vec![ParseError {
            path: Vec::new(),
            message,
        }],
        Status::Aborted,
    ))
}

pub(super) fn strings(
    input: Option<&Value>,
    values: &[&str],
    budget: &mut Budget,
) -> Result<Parsed, ZodJsonUnsupported> {
    budget.charge(
        values.len(),
        values.len().saturating_mul(std::mem::size_of::<Atom<'_>>()),
    )?;
    evaluate(
        input,
        &values
            .iter()
            .map(|value| Atom::String(value))
            .collect::<Vec<_>>(),
        false,
        budget,
    )
}
