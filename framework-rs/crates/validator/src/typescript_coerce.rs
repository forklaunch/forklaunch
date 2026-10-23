//! Opt-in `ForkLaunch` Zod scalar coercion for the JSON value domain.
//!
//! These helpers leave the stricter native `coerce` functions unchanged. They
//! reproduce the repository's `Number(value)` preprocessing and exact boolean
//! string rules, not arbitrary JavaScript objects, Dates, Symbols or functions.
//! Array conversion is iterative, including nested arrays; no recursive stack
//! growth is needed. Errors are public root-level Zod diagnostics; callers may
//! prepend their declared field path before formatting an HTTP response.
use crate::ParseError;
use serde_json::Value;

#[path = "typescript_date.rs"]
mod date;
pub use date::{DateCoercionError, date_epoch_millis};

#[path = "zod_binary.rs"]
mod binary;
pub use binary::zod_binary;
#[path = "zod_string.rs"]
mod strict_string;
pub use strict_string::{
    ZodIpVersion, ZodStringCheck, ZodStringFormat, ZodStringFormatParameterError, ZodStringRule,
    ZodStringStep, ZodTimePrecision, zod_string, zod_string_checks, zod_string_rules,
};

#[path = "zod_multiple_of.rs"]
mod multiple_of;
pub use multiple_of::ZodMultipleOf;
#[path = "zod_float_bound.rs"]
mod float_bound;
pub use float_bound::ZodFloatBound;

#[path = "zod_number.rs"]
mod constrained_number;
pub use constrained_number::{
    ZodIntegerBound, ZodNumberCheck, ZodNumberStep, zod_number, zod_number_checks,
};

#[path = "zod_json.rs"]
mod json;
pub use json::{
    OrderedJsonDocument, OrderedJsonError, ZodJsonEnumValue, ZodJsonField, ZodJsonLiteral,
    ZodJsonNativeEnumEntry, ZodJsonOutcome, ZodJsonSchema, ZodJsonUnknownKeys, ZodJsonUnsupported,
    zod_json, zod_json_at_path, zod_json_ordered, zod_json_ordered_at_path,
};

/// Coerce a JSON value with the actual `ForkLaunch` Zod number preprocessing.
/// `None` represents JavaScript undefined. Infinities and signed zero are
/// preserved, as accepted by Zod v3's unconstrained number schema.
/// # Errors
/// Rejects NaN and JSON objects whose own `toString` field prevents primitive
/// conversion, matching the preprocessing catch and subsequent type check.
pub fn number(value: Option<&Value>) -> Result<f64, ParseError> {
    let converted = match value {
        None => f64::NAN,
        Some(Value::Null) => 0.0,
        Some(Value::Bool(value)) => {
            if *value {
                1.0
            } else {
                0.0
            }
        }
        Some(Value::Number(value)) => value.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(value)) => string_number(value),
        Some(Value::Array(values)) => {
            array_number(values).map_err(|()| error("number", "array"))?
        }
        Some(Value::Object(value)) => {
            if value.contains_key("toString") {
                return Err(error("number", "object"));
            }
            f64::NAN
        }
    };
    if converted.is_nan() {
        Err(error("number", "nan"))
    } else {
        Ok(converted)
    }
}

/// Number coercion followed by Zod v3's integer constraint. The result remains
/// an IEEE-754 number, including rounded integers beyond Rust's i64 range.
/// # Errors
/// Rejects invalid numbers, infinities and fractional values.
pub fn integer_number(value: Option<&Value>) -> Result<f64, ParseError> {
    let value = number(value)?;
    if value.is_finite() && value.fract() == 0.0 {
        Ok(value)
    } else {
        Err(error("integer", "float"))
    }
}

/// Match `ForkLaunch`'s boolean preprocessor: booleans or case-insensitive exact
/// strings `true`/`false`. Whitespace, numbers and truthy objects are rejected.
/// # Errors
/// Returns a root-level public Zod type diagnostic for other values.
pub fn boolean(value: Option<&Value>) -> Result<bool, ParseError> {
    match value {
        None => Err(ParseError {
            path: Vec::new(),
            message: "Required".into(),
        }),
        Some(Value::Bool(value)) => Ok(*value),
        Some(Value::String(value)) if value.eq_ignore_ascii_case("true") => Ok(true),
        Some(Value::String(value)) if value.eq_ignore_ascii_case("false") => Ok(false),
        _ => Err(error(
            "boolean",
            match value {
                None => "undefined",
                Some(Value::Null) => "null",
                Some(Value::Number(_)) => "number",
                Some(Value::String(_)) => "string",
                Some(Value::Array(_)) => "array",
                Some(Value::Object(_)) => "object",
                Some(Value::Bool(_)) => "boolean",
            },
        )),
    }
}

fn error(expected: &str, received: &str) -> ParseError {
    ParseError {
        path: Vec::new(),
        message: format!("Expected {expected}, received {received}"),
    }
}

fn array_number(mut values: &[Value]) -> Result<f64, ()> {
    loop {
        match values {
            [] | [Value::Null] => return Ok(0.0),
            [Value::Array(nested)] => values = nested,
            [Value::String(value)] => return Ok(string_number(value)),
            [Value::Number(value)] => {
                let number = value.as_f64().unwrap_or(f64::NAN);
                return Ok(if number == 0.0 { 0.0 } else { number });
            }
            [Value::Bool(_)] => return Ok(f64::NAN),
            _ => {
                // A comma-separated array cannot become a number, but an
                // unconvertible member throws before Number gets that string.
                let mut pending: Vec<&Value> = values.iter().collect();
                while let Some(value) = pending.pop() {
                    match value {
                        Value::Array(nested) => pending.extend(nested),
                        Value::Object(object) if object.contains_key("toString") => return Err(()),
                        _ => {}
                    }
                }
                return Ok(f64::NAN);
            }
        }
    }
}

fn string_number(value: &str) -> f64 {
    let value = value.trim_matches(ecmascript_whitespace);
    if value.is_empty() {
        return 0.0;
    }
    match value {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    for (prefix, radix, bits) in [
        ("0x", 16, 4),
        ("0X", 16, 4),
        ("0o", 8, 3),
        ("0O", 8, 3),
        ("0b", 2, 1),
        ("0B", 2, 1),
    ] {
        if let Some(digits) = value.strip_prefix(prefix) {
            return radix_number(digits, radix, bits);
        }
    }
    if !decimal(value.as_bytes()) {
        return f64::NAN;
    }
    value.parse().unwrap_or(f64::NAN)
}

fn ecmascript_whitespace(value: char) -> bool {
    matches!(value, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' |
        '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' |
        '\u{205f}' | '\u{3000}' | '\u{feff}')
}

fn decimal(mut bytes: &[u8]) -> bool {
    if matches!(bytes.first(), Some(b'+' | b'-')) {
        bytes = &bytes[1..];
    }
    let whole = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    bytes = &bytes[whole..];
    let mut fraction = 0;
    if bytes.first() == Some(&b'.') {
        bytes = &bytes[1..];
        fraction = bytes
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        bytes = &bytes[fraction..];
    }
    if whole + fraction == 0 {
        return false;
    }
    if matches!(bytes.first(), Some(b'e' | b'E')) {
        bytes = &bytes[1..];
        if matches!(bytes.first(), Some(b'+' | b'-')) {
            bytes = &bytes[1..];
        }
        let exponent = bytes
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        if exponent == 0 {
            return false;
        }
        bytes = &bytes[exponent..];
    }
    bytes.is_empty()
}

// Radices are powers of two. Keep the leading 53 bits, then round once using
// guard/sticky bits. Repeated floating-point multiply/add double-rounds long
// literals and differs from ECMAScript StringNumericLiteral conversion.
fn radix_number(digits: &str, radix: u32, width: u32) -> f64 {
    if digits.is_empty() {
        return f64::NAN;
    }
    let (mut length, mut top, mut guard, mut sticky) = (0_u32, 0_u64, false, false);
    for character in digits.chars() {
        let Some(digit) = character.to_digit(radix).filter(|_| character.is_ascii()) else {
            return f64::NAN;
        };
        for shift in (0..width).rev() {
            let bit = (digit >> shift) & 1;
            if length == 0 && bit == 0 {
                continue;
            }
            length = length.saturating_add(1);
            match length {
                1..=53 => top = (top << 1) | u64::from(bit),
                54 => guard = bit != 0,
                _ => sticky |= bit != 0,
            }
        }
    }
    if length == 0 {
        return 0.0;
    }
    if length > 1024 {
        return f64::INFINITY;
    }
    let mut exponent = length - 1 + 1023;
    if length < 53 {
        top <<= 53 - length;
    }
    if guard && (sticky || top & 1 != 0) {
        top += 1;
        if top == 1_u64 << 53 {
            top >>= 1;
            exponent += 1;
        }
    }
    if exponent >= 2047 {
        return f64::INFINITY;
    }
    f64::from_bits((u64::from(exponent) << 52) | (top & ((1_u64 << 52) - 1)))
}

pub use json::ZodJsonStringCheck;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coerce;
    use forklaunch_common::ContractValidationLocation;
    use serde_json::json;

    #[test]
    fn compatibility_is_opt_in_and_preserves_native_coercion() {
        let location = ContractValidationLocation::Query;
        assert!(coerce::number(location, "value", "").is_err());
        assert_eq!(
            number(Some(&json!(""))).unwrap().to_bits(),
            0.0_f64.to_bits()
        );
        assert!(coerce::integer(location, "value", "0xff").is_err());
        assert_eq!(
            integer_number(Some(&json!("0xff"))).unwrap().to_bits(),
            255.0_f64.to_bits()
        );
        assert!(coerce::boolean(location, "value", " TRUE ").unwrap());
        assert!(boolean(Some(&json!(" TRUE "))).is_err());
        assert!(boolean(Some(&json!(1))).is_err());
    }

    #[test]
    fn array_stringification_preserves_ecmascript_zero_and_type_rules() {
        let direct = Value::Number(serde_json::Number::from_f64(-0.0).unwrap());
        assert_eq!(
            number(Some(&direct)).unwrap().to_bits(),
            (-0.0_f64).to_bits()
        );
        assert_eq!(
            number(Some(&json!([direct]))).unwrap().to_bits(),
            0.0_f64.to_bits()
        );
        assert!(number(Some(&json!([true]))).is_err());
        assert_eq!(
            number(Some(&json!([[null]]))).unwrap().to_bits(),
            0.0_f64.to_bits()
        );
        assert_eq!(
            number(Some(&json!([{"toString":null}])))
                .unwrap_err()
                .message,
            "Expected number, received array"
        );
    }
}
