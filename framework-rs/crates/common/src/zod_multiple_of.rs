//! Zod v3's decimal-string remainder algorithm, including exponent quirks.

/// A runtime Zod v3 multiple-of constraint, preserving all IEEE-754 divisor bits.
/// Zero, negative and nonfinite divisors are accepted as configuration, exactly
/// as Zod does. This is not a JSON Schema `multipleOf` declaration: Zod's checks
/// use JavaScript formatting and may differ from mathematical divisibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZodMultipleOf(u64);

impl ZodMultipleOf {
    #[must_use]
    pub const fn new(divisor: f64) -> Self {
        Self(divisor.to_bits())
    }

    /// Evaluate the exact Zod v3 remainder predicate without coercion.
    #[must_use]
    pub fn accepts(self, value: f64) -> bool {
        let divisor = f64::from_bits(self.0);
        let decimals = decimal_count(value).max(decimal_count(divisor));
        // Zod divides this remainder by 10**decimals. Here both parsed values
        // are integer-valued doubles and decimals is bounded by shortest-f64
        // formatting, so that finite positive scale cannot change zero-ness.
        scaled_integer(value, decimals) % scaled_integer(divisor, decimals) == 0.0
    }

    /// Render the default Zod v3 diagnostic using ECMAScript number spelling.
    #[must_use]
    pub fn diagnostic(self) -> String {
        format!(
            "Number must be a multiple of {}",
            ryu_js::Buffer::new().format(f64::from_bits(self.0))
        )
    }
}

fn decimal_count(value: f64) -> u8 {
    let mut buffer = ryu_js::Buffer::new();
    let count = buffer
        .format(value)
        .split_once('.')
        .map_or(0, |(_, tail)| tail.len());
    // Even exponent-form shortest f64 output is far shorter than 100 digits.
    u8::try_from(count).unwrap_or(100)
}

fn scaled_integer(value: f64, decimals: u8) -> f64 {
    let text = if !value.is_finite() || value.abs() >= 1e21 {
        ryu_js::Buffer::new().format(value).replacen('.', "", 1)
    } else if let Some(rounded) = halfway_scaled_integer(value, decimals) {
        rounded
    } else {
        // Rust's fixed formatting is exact, but ties round to even. The branch
        // above supplies ECMAScript's magnitude-up tie rule instead.
        format!("{:.*}", usize::from(decimals), value).replacen('.', "", 1)
    };
    let sign = usize::from(text.starts_with('-') || text.starts_with('+'));
    let digits = text.as_bytes()[sign..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits == 0 {
        f64::NAN
    } else {
        // Decimal parseInt stops before scientific notation's exponent. Parse
        // the complete integer prefix once, avoiding accumulating rounded f64s.
        text[..sign + digits].parse().unwrap_or(f64::NAN)
    }
}

fn halfway_scaled_integer(value: f64, decimals: u8) -> Option<String> {
    let bits = value.abs().to_bits();
    let exponent = i32::try_from((bits >> 52) & 0x7ff).ok()?;
    let fraction = bits & ((1u64 << 52) - 1);
    let (mantissa, binary_exponent) = if exponent == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1u64 << 52), exponent - 1023 - 52)
    };
    let shift = -(binary_exponent + i32::from(decimals));
    if !(1..128).contains(&shift) {
        return None;
    }
    // Shortest f64 spelling bounds decimals below 24, so this product fits
    // u128. Checked arithmetic keeps this helper safe if its caller changes.
    let numerator = u128::from(mantissa).checked_mul(5u128.checked_pow(u32::from(decimals))?)?;
    let denominator = 1u128.checked_shl(u32::try_from(shift).ok()?)?;
    if numerator % denominator != denominator / 2 {
        return None;
    }
    let rounded = numerator / denominator + 1;
    Some(if value.is_sign_negative() {
        format!("-{rounded}")
    } else {
        rounded.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::float_cmp)] // Exact rounded integer equality is the contract.
    fn fixed_decimal_rounding_preserves_tiny_values_and_javascript_ties() {
        assert_eq!(scaled_integer(1.738_752_315_208_379_6e-18, 20), 174.0);
        assert_eq!(scaled_integer(1.25, 1), 13.0);
        assert_eq!(scaled_integer(-1.25, 1), -13.0);
        assert_eq!(scaled_integer(1.75, 1), 18.0);
        assert_eq!(
            scaled_integer(1.000_000_000_000_000_1e21, 20),
            10_000_000_000_000_001.0
        );
    }

    #[test]
    fn preserves_source_exponent_quirks_and_all_divisor_registrations() {
        for divisor in [0.0, -0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let constraint = ZodMultipleOf::new(divisor);
            for value in [0.0, -0.0, 1.0, f64::INFINITY] {
                assert!(!constraint.accepts(value));
            }
        }
        assert!(!ZodMultipleOf::new(1e-7).accepts(1e-7));
        assert!(!ZodMultipleOf::new(1e-7).accepts(0.0));
        assert!(ZodMultipleOf::new(1e21).accepts(0.1));
        assert!(ZodMultipleOf::new(-0.1).accepts(0.3));
        assert!(!ZodMultipleOf::new(0.1).accepts(0.1 + 0.2));
        assert_eq!(
            ZodMultipleOf::new(-0.0).diagnostic(),
            "Number must be a multiple of 0"
        );
        assert_eq!(
            ZodMultipleOf::new(f64::NAN).diagnostic(),
            "Number must be a multiple of NaN"
        );
    }
}
