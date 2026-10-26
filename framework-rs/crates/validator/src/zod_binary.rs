//! Binary factory conversion using Node's permissive base64 string semantics.
use crate::ParseError;
use serde_json::Value;

/// Match the `ForkLaunch` Zod binary factory over JSON inputs.
/// A strict string is decoded into bytes, accepting both base64 alphabets,
/// ignoring nonalphabet bytes, stopping at the first padding byte, and ignoring
/// incomplete final sextets. As Node does, UTF-16 code units are truncated to
/// their low byte before decoding; this is not UTF-8 base64 normalization.
///
/// This is a byte conversion, not a canonical-base64 validator. The original
/// input stays unchanged. Output is explicitly bytes, not a JSON string, Blob,
/// or JavaScript typed-array object. Lone UTF-16 surrogates cannot be represented
/// by the input JSON `Value` string domain. Existing native codecs are unchanged.
/// # Errors
/// Returns the same strict string type diagnostic as the source Zod factory.
pub fn zod_binary(input: Option<&Value>) -> Result<Vec<u8>, Vec<ParseError>> {
    let value = super::zod_string(input, &[])?;
    let mut output = Vec::new();
    let mut pending = 0u16;
    let mut bits = 0u8;
    for unit in value.encode_utf16() {
        let byte = unit.to_le_bytes()[0];
        let digit = match byte {
            b'=' => break,
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => continue,
        };
        pending = (pending << 6) | u16::from(digit);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((pending >> bits).to_le_bytes()[0]);
            pending &= (1 << bits) - 1;
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn preserves_source_low_bytes_padding_and_raw_input() {
        let value = json!(" Y!Q\n==Yg==");
        assert_eq!(zod_binary(Some(&value)).unwrap(), b"a");
        assert_eq!(value, json!(" Y!Q\n==Yg=="));
        assert_eq!(zod_binary(Some(&json!("ŁŁ"))).unwrap(), [0]);
        assert!(zod_binary(Some(&json!("A💡A"))).unwrap().is_empty());
        assert_eq!(zod_binary(Some(&json!("____"))).unwrap(), [255; 3]);
        assert_eq!(zod_binary(None).unwrap_err()[0].message, "Required");
    }
}
