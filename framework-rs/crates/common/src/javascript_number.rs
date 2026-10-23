//! JavaScript Number text for protocol fields that accept the full IEEE domain.
/// Format a number like JavaScript `Number.prototype.toString`, including signed
/// zero, NaN and infinities. This is text, not JSON numeric serialization.
#[must_use]
pub fn to_string(value: f64) -> String {
    ryu_js::Buffer::new().format(value).to_owned()
}
#[cfg(test)]
mod tests {
    #[test]
    fn ieee_protocol_text() {
        for (value, expected) in [
            (f64::NAN, "NaN"),
            (f64::INFINITY, "Infinity"),
            (f64::NEG_INFINITY, "-Infinity"),
            (-0.0, "0"),
            (1e21, "1e+21"),
            (0.5, "0.5"),
        ] {
            assert_eq!(super::to_string(value), expected);
        }
    }
}
