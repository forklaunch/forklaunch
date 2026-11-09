//! Ordered Zod v3 number constraints after `ForkLaunch` number preprocessing.
use super::{ZodFloatBound, ZodMultipleOf};
use crate::ParseError;
use serde_json::Value;

const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// A bound whose integer value and decimal diagnostic are exact in JavaScript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZodIntegerBound(i64);
impl ZodIntegerBound {
    /// Returns `None` outside JavaScript's safe integer range.
    #[must_use]
    pub const fn new(value: i64) -> Option<Self> {
        if value >= -MAX_SAFE_INTEGER && value <= MAX_SAFE_INTEGER {
            Some(Self(value))
        } else {
            None
        }
    }
}

/// Ordered constraints for the repository's Zod v3 number preprocessing.
/// Original integer-bound variants retain their safe-integer registration limits.
/// Explicit float-bound variants admit all IEEE-754 bounds, including NaN.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZodNumberStep {
    Int,
    Finite,
    /// Checks only magnitude, as Zod v3 does; this does not require an integer.
    Safe,
    Min(ZodIntegerBound),
    Max(ZodIntegerBound),
    Gt(ZodIntegerBound),
    Lt(ZodIntegerBound),
    MultipleOf(ZodMultipleOf),
    MinFloat(ZodFloatBound),
    MaxFloat(ZodFloatBound),
    GtFloat(ZodFloatBound),
    LtFloat(ZodFloatBound),
}

/// One ordered runtime constraint with an optional literal diagnostic override.
/// `Some("")` preserves an empty message. Overrides never replace preprocessing
/// or type errors, and do not execute arbitrary error-map callbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZodNumberCheck<'a> {
    pub step: ZodNumberStep,
    pub message: Option<&'a str>,
}

/// Apply ordered constraints after the existing JavaScript number coercion.
/// This matches `number.pipe(number.innerType().checks...)` using the actual
/// `ForkLaunch` Zod v3 factory. Coercion failures stop checks; check failures
/// accumulate in order. Successful output retains IEEE-754 bits, including
/// negative zero and infinities unless a constraint excludes them.
///
/// This is a bounded helper, not a general Zod or JSON Schema interpreter.
/// Use [`zod_number_checks`] for literal per-check messages. General error maps,
/// refinements and arbitrary JavaScript objects are not implemented. Successful
/// nonfinite results cannot be represented as a JSON number; callers must not
/// silently serialize them as null. Existing native validation stays unchanged.
/// # Errors
/// Returns default Zod v3 public diagnostics at the root path.
pub fn zod_number(input: Option<&Value>, steps: &[ZodNumberStep]) -> Result<f64, Vec<ParseError>> {
    evaluate(
        input,
        steps.iter().copied().map(|step| ZodNumberCheck {
            step,
            message: None,
        }),
    )
}

/// Apply ordered checks with literal per-constraint diagnostic messages.
/// Success retains the original coerced number; failure collects all check
/// messages in declaration order. Preprocessing errors are never overridden.
/// # Errors
/// Returns ordered root-level parse errors, including exact empty messages.
pub fn zod_number_checks(
    input: Option<&Value>,
    checks: &[ZodNumberCheck<'_>],
) -> Result<f64, Vec<ParseError>> {
    evaluate(input, checks.iter().copied())
}

fn evaluate<'a>(
    input: Option<&Value>,
    checks: impl IntoIterator<Item = ZodNumberCheck<'a>>,
) -> Result<f64, Vec<ParseError>> {
    let value = super::number(input).map_err(|error| vec![error])?;
    let mut errors = Vec::new();
    for check in checks {
        let start = errors.len();
        match check.step {
            ZodNumberStep::MultipleOf(limit) if !limit.accepts(value) => {
                errors.push(diagnostic(limit.diagnostic()));
            }
            ZodNumberStep::MinFloat(limit) => float_bound(value, limit, true, true, &mut errors),
            ZodNumberStep::MaxFloat(limit) => float_bound(value, limit, false, true, &mut errors),
            ZodNumberStep::GtFloat(limit) => float_bound(value, limit, true, false, &mut errors),
            ZodNumberStep::LtFloat(limit) => float_bound(value, limit, false, false, &mut errors),
            ZodNumberStep::Int if !value.is_finite() || value.fract() != 0.0 => {
                errors.push(diagnostic("Expected integer, received float"));
            }
            ZodNumberStep::Finite if !value.is_finite() => {
                errors.push(diagnostic("Number must be finite"));
            }
            ZodNumberStep::Safe => {
                bound(value, -MAX_SAFE_INTEGER, true, true, &mut errors);
                bound(value, MAX_SAFE_INTEGER, false, true, &mut errors);
            }
            ZodNumberStep::Min(limit) => bound(value, limit.0, true, true, &mut errors),
            ZodNumberStep::Max(limit) => bound(value, limit.0, false, true, &mut errors),
            ZodNumberStep::Gt(limit) => bound(value, limit.0, true, false, &mut errors),
            ZodNumberStep::Lt(limit) => bound(value, limit.0, false, false, &mut errors),
            _ => {}
        }
        if let Some(message) = check.message {
            for error in &mut errors[start..] {
                message.clone_into(&mut error.message);
            }
        }
    }
    if errors.is_empty() {
        Ok(value)
    } else {
        Err(errors)
    }
}
fn float_bound(
    value: f64,
    limit: ZodFloatBound,
    minimum: bool,
    inclusive: bool,
    errors: &mut Vec<ParseError>,
) {
    if let Some(message) = limit.rejection(value, minimum, inclusive) {
        errors.push(diagnostic(message));
    }
}
fn diagnostic(message: impl Into<String>) -> ParseError {
    ParseError {
        path: Vec::new(),
        message: message.into(),
    }
}
// Bounds are exact safe integers; exclusive checks intentionally reject equality.
#[allow(clippy::cast_precision_loss, clippy::float_cmp)]
fn bound(value: f64, limit: i64, minimum: bool, inclusive: bool, errors: &mut Vec<ParseError>) {
    if if minimum {
        value < limit as f64 || !inclusive && value == limit as f64
    } else {
        value > limit as f64 || !inclusive && value == limit as f64
    } {
        let comparison = if minimum { "greater" } else { "less" };
        let equality = if inclusive { " or equal to" } else { "" };
        errors.push(diagnostic(format!(
            "Number must be {comparison} than{equality} {limit}"
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn literal_messages_do_not_override_preprocessing_or_adjacent_checks() {
        let checks = [
            ZodNumberCheck {
                step: ZodNumberStep::Int,
                message: Some(""),
            },
            ZodNumberCheck {
                step: ZodNumberStep::Finite,
                message: Some("finite\n💡\0"),
            },
        ];
        let errors = zod_number_checks(Some(&json!("Infinity")), &checks).unwrap_err();
        assert_eq!(errors[0].message, "");
        assert_eq!(errors[1].message, "finite\n💡\0");
        assert_eq!(
            zod_number_checks(Some(&json!("NaN")), &checks),
            zod_number(Some(&json!("NaN")), &[])
        );
    }

    #[test]
    fn constraints_preserve_order_and_safe_does_not_mean_integer() {
        assert_eq!(
            zod_number(Some(&json!(0.5)), &[ZodNumberStep::Safe]),
            Ok(0.5)
        );
        let errors = zod_number(
            Some(&json!("Infinity")),
            &[
                ZodNumberStep::Int,
                ZodNumberStep::Finite,
                ZodNumberStep::Safe,
            ],
        )
        .unwrap_err();
        assert_eq!(
            errors
                .iter()
                .map(|error| error.message.as_str())
                .collect::<Vec<_>>(),
            [
                "Expected integer, received float",
                "Number must be finite",
                "Number must be less than or equal to 9007199254740991"
            ]
        );
        assert_eq!(
            zod_number(Some(&json!("-0")), &[]).unwrap().to_bits(),
            (-0.0f64).to_bits()
        );
        assert_eq!(
            zod_number(
                Some(&json!("NaN")),
                &[ZodNumberStep::Int, ZodNumberStep::Finite]
            )
            .unwrap_err()
            .len(),
            1
        );
    }
    #[test]
    fn bounds_require_exact_javascript_integers() {
        assert!(ZodIntegerBound::new(MAX_SAFE_INTEGER).is_some());
        assert!(ZodIntegerBound::new(-MAX_SAFE_INTEGER).is_some());
        for limit in [
            MAX_SAFE_INTEGER + 1,
            -MAX_SAFE_INTEGER - 1,
            i64::MIN,
            i64::MAX,
        ] {
            assert!(ZodIntegerBound::new(limit).is_none());
        }
    }
}
