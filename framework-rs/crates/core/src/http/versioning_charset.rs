//! Decode request character sets while retaining original wire bytes elsewhere.
use super::{ContractValidationError, ContractValidationLocation};
use std::borrow::Cow;

pub(super) fn unsupported() -> ContractValidationError {
    ContractValidationError::new(
        ContractValidationLocation::BodyEncoding,
        "unsupported request charset",
    )
}

pub(super) fn charset(content_type: Option<&str>, default: &str) -> String {
    content_type
        .and_then(|value| value.parse::<mime::Mime>().ok())
        .and_then(|mime| {
            mime.get_param(mime::CHARSET)
                .map(|value| value.as_str().to_ascii_lowercase())
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_ascii_lowercase())
}

pub(super) fn decode<'a>(
    bytes: &'a [u8],
    charset: &str,
) -> Result<Cow<'a, str>, ContractValidationError> {
    let normalized: String = charset
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    let text = match normalized.as_str() {
        "utf8" | "unicode11utf8" => return Ok(super::decode_utf8(bytes)),
        "latin1" | "iso88591" | "binary" => bytes.iter().map(|byte| char::from(*byte)).collect(),
        "ascii" | "usascii" => bytes
            .iter()
            .map(|byte| {
                if byte.is_ascii() {
                    char::from(*byte)
                } else {
                    '�'
                }
            })
            .collect(),
        "utf16" | "utf16le" | "utf16be" | "ucs2" | "ucs2le" | "ucs2be" => {
            let be = if normalized == "utf16" {
                detect_utf16_be(bytes)
            } else {
                normalized.ends_with("be")
            };
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| {
                    if be {
                        u16::from_be_bytes([pair[0], pair[1]])
                    } else {
                        u16::from_le_bytes([pair[0], pair[1]])
                    }
                })
                .collect();
            String::from_utf16_lossy(&units)
        }
        "utf32" | "utf32le" | "utf32be" => {
            let be =
                normalized.ends_with("be") || (normalized == "utf32" && detect_utf32_be(bytes));
            let mut decoded: String = bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|word| {
                    let word = [word[0], word[1], word[2], word[3]];
                    char::from_u32(if be {
                        u32::from_be_bytes(word)
                    } else {
                        u32::from_le_bytes(word)
                    })
                    .unwrap_or('�')
                })
                .collect();
            if !bytes.len().is_multiple_of(4) {
                decoded.push('�');
            }
            decoded
        }
        _ => {
            let encoding =
                encoding_rs::Encoding::for_label(charset.as_bytes()).ok_or_else(unsupported)?;
            if matches!(
                encoding.name(),
                "Shift_JIS" | "GBK" | "gb18030" | "Big5" | "EUC-JP" | "EUC-KR"
            ) {
                decode_multibyte(bytes, encoding)
            } else {
                encoding.decode_without_bom_handling(bytes).0.into_owned()
            }
        }
    };
    Ok(Cow::Owned(
        text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned(),
    ))
}

fn detect_utf16_be(bytes: &[u8]) -> bool {
    if bytes.starts_with(&[0xfe, 0xff]) {
        return true;
    }
    if bytes.starts_with(&[0xff, 0xfe]) {
        return false;
    }
    let (mut le, mut be) = (0, 0);
    for pair in bytes.as_chunks::<2>().0.iter().take(100) {
        if pair[0] == 0 && pair[1] != 0 {
            be += 1;
        }
        if pair[0] != 0 && pair[1] == 0 {
            le += 1;
        }
    }
    be > le
}

// iconv retries the byte following a failed multibyte lead. WHATWG decoders can
// instead consume both bytes as one error, which changes the parsed text.
fn decode_multibyte(mut bytes: &[u8], encoding: &'static encoding_rs::Encoding) -> String {
    let mut output = String::new();
    while !bytes.is_empty() {
        let ascii = bytes
            .iter()
            .position(|byte| !byte.is_ascii())
            .unwrap_or(bytes.len());
        if ascii > 0 {
            output.push_str(std::str::from_utf8(&bytes[..ascii]).expect("ASCII prefix"));
            bytes = &bytes[ascii..];
            continue;
        }
        let mut consumed = 0;
        for length in 1..=bytes.len().min(4) {
            if let Some(text) =
                encoding.decode_without_bom_handling_and_without_replacement(&bytes[..length])
            {
                output.push_str(&text);
                consumed = length;
                break;
            }
        }
        if consumed == 0 {
            output.push('�');
            consumed = 1;
        }
        bytes = &bytes[consumed..];
    }
    output
}

fn detect_utf32_be(bytes: &[u8]) -> bool {
    if bytes.starts_with(&[0, 0, 0xfe, 0xff]) {
        return true;
    }
    if bytes.starts_with(&[0xff, 0xfe, 0, 0]) {
        return false;
    }
    let (mut le, mut be) = (0_i32, 0_i32);
    for word in bytes.as_chunks::<4>().0.iter().take(100) {
        if word[0] != 0 || word[1] > 0x10 {
            be -= 1;
        }
        if word[3] != 0 || word[2] > 0x10 {
            le -= 1;
        }
        if word[0] == 0 && word[1] == 0 && (word[2] != 0 || word[3] != 0) {
            be += 1;
        }
        if word[2] == 0 && word[3] == 0 && (word[0] != 0 || word[1] != 0) {
            le += 1;
        }
    }
    be > le
}
