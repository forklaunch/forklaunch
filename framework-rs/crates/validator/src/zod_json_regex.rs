//! Ordered borrowed checks and allocating transforms with shared resource limits.
use super::super::{
    ZodStringCheck, ZodStringRule, ZodStringStep, strict_string, zod_string_checks,
};
use super::{Budget, Parsed, Status, ZodJsonUnsupported};
use crate::ParseError;
#[cfg(feature = "zod-regex")]
use forklaunch_common::zod_regex::{RegexMatchLimits, ZodRegex};
use std::borrow::Cow;
#[path = "zod_string_case.rs"]
mod casing;
use serde_json::Value;

/// Ordered scalar checks and Unicode16 allocating case transformations.
/// Optional regex ownership/admission remains with the caller; this borrows it.
/// Conversion targets Node24.2 Unicode16 default casing over valid UTF-8;
/// locale-specific casing, normalization and lone surrogates are not represented.
#[derive(Clone, Copy, Debug)]
pub enum ZodJsonStringCheck<'a> {
    Scalar(ZodStringCheck<'a>),
    /// Unicode16 locale-independent conversion; no normalization is performed.
    Lowercase,
    /// Unicode16 locale-independent conversion; may expand multiple scalars.
    Uppercase,
    #[cfg(feature = "zod-regex")]
    Regex {
        compiled: &'a ZodRegex,
        limits: RegexMatchLimits,
        message: Option<&'a str>,
    },
}
impl ZodJsonStringCheck<'_> {
    pub(super) fn bytes(self) -> usize {
        match self {
            Self::Lowercase | Self::Uppercase => 0,
            #[cfg(feature = "zod-regex")]
            Self::Regex { message, .. } => message.map_or(0, str::len),
            Self::Scalar(check) => {
                check
                    .message
                    .map_or(0, str::len)
                    .saturating_add(match check.rule {
                        ZodStringRule::StartsWith(value)
                        | ZodStringRule::EndsWith(value)
                        | ZodStringRule::Includes { needle: value, .. } => value.len(),
                        ZodStringRule::Jwt { algorithm } => algorithm.map_or(0, str::len),
                        _ => 0,
                    })
            }
        }
    }
}
pub(super) fn evaluate(
    input: Option<&Value>,
    checks: &[ZodJsonStringCheck<'_>],
    budget: &mut Budget,
) -> Result<Parsed, ZodJsonUnsupported> {
    let mut value = match zod_string_checks(input, &[]) {
        Ok(value) => Cow::Borrowed(value),
        Err(errors) => return Ok(Parsed::invalid(errors, Status::Aborted)),
    };
    let mut length = value.encode_utf16().count() as u64;
    let mut errors = Vec::new();
    for check in checks {
        budget.charge(
            0,
            value
                .len()
                .saturating_add(check.bytes())
                .saturating_add(128),
        )?;
        match *check {
            ZodJsonStringCheck::Lowercase | ZodJsonStringCheck::Uppercase => {
                value = Cow::Owned(casing::convert(
                    &value,
                    matches!(check, ZodJsonStringCheck::Uppercase),
                    budget,
                )?);
                length = value.encode_utf16().count() as u64;
            }
            ZodJsonStringCheck::Scalar(check) => {
                if let Some(error) = scalar(&mut value, &mut length, check) {
                    errors.push(error);
                }
            }
            #[cfg(feature = "zod-regex")]
            ZodJsonStringCheck::Regex {
                compiled,
                limits,
                message,
            } => {
                let work = limits
                    .input_units
                    .checked_mul(2)
                    .and_then(|v| v.checked_add(1))
                    .and_then(|v| v.checked_add(limits.work))
                    .ok_or(ZodJsonUnsupported::RegexResource)?;
                budget.regex_work = budget
                    .regex_work
                    .checked_sub(work)
                    .ok_or(ZodJsonUnsupported::RegexResource)?;
                if !compiled
                    .test(&value, limits)
                    .map_err(|_| ZodJsonUnsupported::RegexResource)?
                {
                    errors.push(ParseError {
                        path: Vec::new(),
                        message: message.unwrap_or("Invalid").to_owned(),
                    });
                }
            }
        }
    }
    if matches!(value, Cow::Borrowed(_)) {
        budget.charge(0, value.len())?;
    }
    Ok(if errors.is_empty() {
        Parsed::valid(Some(Value::String(value.into_owned())))
    } else {
        let mut parsed = Parsed::invalid(errors, Status::Dirty);
        if budget.track_origins {
            parsed.value = Some(Value::String(value.into_owned()));
        }
        parsed
    })
}

fn scalar(
    value: &mut Cow<'_, str>,
    length: &mut u64,
    check: ZodStringCheck<'_>,
) -> Option<ParseError> {
    if check.rule == ZodStringRule::Step(ZodStringStep::Trim) {
        match value {
            Cow::Borrowed(text) => *text = text.trim_matches(super::super::ecmascript_whitespace),
            Cow::Owned(text) => {
                let trimmed = text.trim_matches(super::super::ecmascript_whitespace);
                let start = trimmed.as_ptr() as usize - text.as_ptr() as usize;
                let end = start + trimmed.len();
                text.truncate(end);
                text.drain(..start);
            }
        }
        *length = value.encode_utf16().count() as u64;
        None
    } else {
        strict_string::apply_check(&mut value.as_ref(), length, check)
    }
}

#[cfg(all(test, feature = "zod-regex"))]
mod tests {
    use super::*;
    use crate::typescript_coerce::{ZodJsonOutcome, ZodJsonSchema, ZodStringStep, zod_json};
    use forklaunch_common::zod_regex::RegexCompileLimits;
    use serde_json::json;
    fn pattern(source: &str) -> ZodRegex {
        ZodRegex::compile(
            &source.encode_utf16().collect::<Vec<_>>(),
            "",
            RegexCompileLimits::default(),
        )
        .unwrap()
    }
    #[test]
    fn transforms_and_diagnostics_are_ordered_and_atomic() {
        let regex = pattern("^a$");
        let check = ZodJsonStringCheck::Regex {
            compiled: &regex,
            limits: RegexMatchLimits::default(),
            message: Some("regex"),
        };
        let trim = ZodJsonStringCheck::Scalar(ZodStringCheck {
            rule: ZodStringRule::Step(ZodStringStep::Trim),
            message: None,
        });
        let input = json!(" a ");
        assert_eq!(
            zod_json(
                Some(&input),
                &ZodJsonSchema::CheckedString(vec![trim, check])
            )
            .unwrap(),
            ZodJsonOutcome::Valid(Some(json!("a")))
        );
        let ZodJsonOutcome::Invalid(errors) = zod_json(
            Some(&input),
            &ZodJsonSchema::CheckedString(vec![check, trim, check]),
        )
        .unwrap() else {
            panic!("expected invalid")
        };
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].message, "regex");
        assert_eq!(input, json!(" a "));
    }
    #[test]
    fn regex_observes_allocating_transforms_in_declared_order() {
        let regex = pattern("^SS$");
        let check = ZodJsonStringCheck::Regex {
            compiled: &regex,
            limits: RegexMatchLimits::default(),
            message: None,
        };
        let input = json!("ß");
        assert_eq!(
            zod_json(
                Some(&input),
                &ZodJsonSchema::CheckedString(vec![ZodJsonStringCheck::Uppercase, check])
            )
            .unwrap(),
            ZodJsonOutcome::Valid(Some(json!("SS")))
        );
        assert!(matches!(
            zod_json(
                Some(&input),
                &ZodJsonSchema::CheckedString(vec![check, ZodJsonStringCheck::Uppercase])
            )
            .unwrap(),
            ZodJsonOutcome::Invalid(_)
        ));
    }
    #[test]
    fn operational_failure_cannot_be_swallowed_by_union() {
        let regex = pattern("^(a+)+$");
        let schema = ZodJsonSchema::Union(vec![
            ZodJsonSchema::CheckedString(vec![ZodJsonStringCheck::Regex {
                compiled: &regex,
                limits: RegexMatchLimits {
                    work: 1000,
                    ..Default::default()
                },
                message: None,
            }]),
            ZodJsonSchema::Unknown,
        ]);
        assert_eq!(
            zod_json(Some(&json!(format!("{}!", "a".repeat(40)))), &schema),
            Err(ZodJsonUnsupported::RegexResource)
        );
    }
    #[test]
    fn array_items_share_aggregate_work() {
        let regex = pattern("a");
        let schema = ZodJsonSchema::Array {
            element: Box::new(ZodJsonSchema::CheckedString(vec![
                ZodJsonStringCheck::Regex {
                    compiled: &regex,
                    limits: RegexMatchLimits::default(),
                    message: None,
                },
            ])),
            checks: vec![],
        };
        assert!(matches!(
            zod_json(Some(&json!(["a", "a"])), &schema),
            Ok(ZodJsonOutcome::Valid(_))
        ));
        assert_eq!(
            zod_json(Some(&json!(vec!["a"; 40])), &schema),
            Err(ZodJsonUnsupported::RegexResource)
        );
    }
}

#[cfg(test)]
mod case_tests {
    use super::*;
    use crate::typescript_coerce::{ZodJsonOutcome, ZodJsonSchema, zod_json};
    use serde_json::json;
    fn step(step: ZodStringStep) -> ZodJsonStringCheck<'static> {
        ZodJsonStringCheck::Scalar(ZodStringCheck {
            rule: ZodStringRule::Step(step),
            message: None,
        })
    }
    #[test]
    fn allocating_transforms_preserve_order_and_atomic_dirty_intersection() {
        let schema = |checks| ZodJsonSchema::CheckedString(checks);
        let input = json!(" ß ");
        assert_eq!(
            zod_json(
                Some(&input),
                &schema(vec![
                    ZodJsonStringCheck::Uppercase,
                    step(ZodStringStep::Trim),
                    step(ZodStringStep::Length(2))
                ])
            )
            .unwrap(),
            ZodJsonOutcome::Valid(Some(json!("SS")))
        );
        assert_eq!(input, json!(" ß "));
        let dirty = schema(vec![
            step(ZodStringStep::Max(0)),
            ZodJsonStringCheck::Uppercase,
        ]);
        let right = schema(vec![ZodJsonStringCheck::Uppercase]);
        let ZodJsonOutcome::Invalid(errors) = zod_json(
            Some(&json!("ß")),
            &ZodJsonSchema::Intersection {
                left: Box::new(dirty),
                right: Box::new(right),
            },
        )
        .unwrap() else {
            panic!("invalid")
        };
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("at most 0"));
        let ZodJsonOutcome::Invalid(errors) = zod_json(
            Some(&json!("ß")),
            &schema(vec![
                ZodJsonStringCheck::Uppercase,
                step(ZodStringStep::Max(1)),
            ]),
        )
        .unwrap() else {
            panic!("invalid")
        };
        assert_eq!(errors.len(), 1);
    }
    #[test]
    fn repeated_expanding_case_work_cannot_be_swallowed_by_union() {
        let checks = vec![ZodJsonStringCheck::Uppercase; 100];
        let schema = ZodJsonSchema::Union(vec![
            ZodJsonSchema::CheckedString(checks),
            ZodJsonSchema::Unknown,
        ]);
        assert_eq!(
            zod_json(Some(&json!("ß".repeat(100_000))), &schema),
            Err(ZodJsonUnsupported::EvaluationBytes)
        );
    }
}
