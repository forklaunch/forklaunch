//! Typed public literal inputs; equality uses JavaScript's strict scalar rules.
use serde_json::Value;

/// Literal values supported by the typed `ForkLaunch` validator factory.
/// Numbers use strict IEEE-754 equality: signed zeros compare equal, NaN never
/// matches. Successful evaluation preserves the input's sign. Nonfinite expected
/// numbers are allowed, but input remains restricted to the evaluator's finite
/// JSON domain. Explicit null/undefined variants cover raw Zod literal schemas;
/// undefined means absent input. Object reflection and symbols are not implied.
/// Ordered enums lower to `ZodJsonSchema::Union` containing literal branches.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZodJsonLiteral<'a> {
    String(&'a str),
    Number(f64),
    Boolean(bool),
    Null,
    Undefined,
}
impl ZodJsonLiteral<'_> {
    pub(super) fn matches(self, input: Option<&Value>) -> bool {
        match (self, input) {
            (Self::Null, Some(Value::Null)) | (Self::Undefined, None) => true,
            (Self::String(expected), Some(Value::String(value))) => value == expected,
            (Self::Boolean(expected), Some(Value::Bool(value))) => *value == expected,
            (Self::Number(expected), Some(Value::Number(value))) => {
                value.as_f64() == Some(expected)
            }
            _ => false,
        }
    }

    pub(super) fn bytes(self) -> usize {
        match self {
            Self::String(value) => value.len(),
            _ => 0,
        }
    }

    pub(super) fn diagnostic(self) -> String {
        let expected = match self {
            Self::String(value) => serde_json::to_string(value).expect("strings serialize to JSON"),
            Self::Number(value) if value.is_finite() => {
                ryu_js::Buffer::new().format(value).to_owned()
            }
            Self::Number(_) | Self::Null => "null".into(),
            Self::Boolean(value) => value.to_string(),
            Self::Undefined => "undefined".into(),
        };
        format!("Invalid literal value, expected {expected}")
    }
}
