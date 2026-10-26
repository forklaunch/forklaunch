//! Bounded URL-encoded matching for strings, arrays and nested bracket paths.
use super::{ContractValidationError, body_too_large, invalid_body};
use serde_json::Value;
#[path = "versioning_form_tree.rs"]
mod tree;

#[cfg(test)]
fn parse(bytes: &[u8]) -> Result<Value, ContractValidationError> {
    parse_decoded(
        &super::decode_utf8(bytes),
        false,
        &forklaunch_core::contract::BodyParserOptions::default(),
    )
}
pub(super) fn parse_decoded(
    text: &str,
    latin1: bool,
    options: &forklaunch_core::contract::BodyParserOptions,
) -> Result<Value, ContractValidationError> {
    let parameter_count = text.bytes().filter(|byte| *byte == b'&').count() + 1;
    if parameter_count > options.form_parameter_limit {
        return Err(body_too_large("form exceeds parameter limit"));
    }
    let mut latin1 = latin1;
    let sentinel = if options.form_charset_sentinel {
        text.split('&')
            .enumerate()
            .find(|(_, part)| part.starts_with("utf8="))
            .map(|(index, part)| {
                match part {
                    "utf8=%E2%9C%93" => latin1 = false,
                    "utf8=%26%2310003%3B" => latin1 = true,
                    _ => {}
                }
                index
            })
    } else {
        None
    };
    let limit = u64::try_from(parameter_count.max(100)).expect("bounded parameter count");
    let mut groups = std::collections::BTreeMap::<String, (usize, Vec<Value>)>::new();
    for (index, part) in text.split('&').enumerate() {
        if sentinel == Some(index) {
            continue;
        }
        if index >= options.form_parameter_limit {
            return Err(body_too_large("form exceeds parameter limit"));
        }
        let part = brackets(part);
        let separator = part
            .find("]=")
            .map(|index| index + 1)
            .or_else(|| part.find('='));
        let (key, value) = separator.map_or((part.as_str(), ""), |index| {
            (&part[..index], &part[index + 1..])
        });
        let key = decode(key, latin1);
        let mut value = decode(value, latin1);
        if latin1 && options.form_interpret_numeric_entities {
            value = numeric_entities(&value);
        }
        groups
            .entry(key)
            .or_insert_with(|| (index, Vec::new()))
            .1
            .push(Value::String(value));
    }
    let mut groups: Vec<_> = groups.into_iter().collect();
    // Object.keys enumerates canonical JavaScript integer keys before strings.
    groups.sort_by_key(|(key, (order, _))| {
        let integer = key
            .parse::<u32>()
            .ok()
            .filter(|n| *n != u32::MAX && n.to_string() == *key);
        (integer.is_none(), integer.unwrap_or(0), *order)
    });
    let mut fields = tree::Node::empty();
    for (key, (_, values)) in groups {
        let node = tree::wrap(
            &key,
            tree::Node::values(values),
            limit,
            if options.form_extended {
                options.form_depth
            } else {
                0
            },
        )
        .ok_or_else(|| invalid_body("form exceeds nesting depth"))?;
        fields = fields.merge(node, limit);
        if !fields.within_depth(0) {
            return Err(invalid_body("form exceeds merged nesting depth"));
        }
    }
    fields
        .into_value(0)
        .ok_or_else(|| invalid_body("form exceeds merged nesting depth"))
}

// qs uses decodeURIComponent: malformed percent escapes or UTF-8 retain the
// entire original component, after '+' replacement, rather than lossy decoding.
fn decode(encoded: &str, latin1: bool) -> String {
    if latin1 {
        return decode_latin1(encoded);
    }
    let replaced = brackets(encoded).replace('+', " ");
    let input = replaced.as_bytes();
    let mut decoded = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if input[index] == b'%' {
            let Some(pair) = input.get(index + 1..index + 3) else {
                return replaced;
            };
            let Some(high) = digit(pair[0]) else {
                return replaced;
            };
            let Some(low) = digit(pair[1]) else {
                return replaced;
            };
            decoded.push(high * 16 + low);
            index += 3;
        } else {
            decoded.push(input[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).unwrap_or(replaced)
}
fn decode_latin1(encoded: &str) -> String {
    let replaced = brackets(encoded).replace('+', " ");
    let mut output = String::new();
    let mut remaining = replaced.as_str();
    while !remaining.is_empty() {
        let bytes = remaining.as_bytes();
        if bytes[0] == b'%'
            && bytes.len() >= 3
            && let (Some(high), Some(low)) = (digit(bytes[1]), digit(bytes[2]))
        {
            output.push(char::from(high * 16 + low));
            remaining = &remaining[3..];
            continue;
        }
        let character = remaining.chars().next().expect("nonempty remainder");
        output.push(character);
        remaining = &remaining[character.len_utf8()..];
    }
    output
}
fn brackets(encoded: &str) -> String {
    encoded
        .replace("%5B", "[")
        .replace("%5b", "[")
        .replace("%5D", "]")
        .replace("%5d", "]")
}
const fn digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn numeric_entities(mut text: &str) -> String {
    let mut units = Vec::with_capacity(text.len());
    while let Some(start) = text.find("&#") {
        units.extend(text[..start].encode_utf16());
        let tail = &text[start + 2..];
        let count = tail.bytes().take_while(u8::is_ascii_digit).count();
        if count > 0 && tail.as_bytes().get(count) == Some(&b';') {
            units.push(entity_code_unit(&tail[..count]));
            text = &tail[count + 1..];
        } else {
            units.extend("&#".encode_utf16());
            text = tail;
        }
    }
    units.extend(text.encode_utf16());
    String::from_utf16_lossy(&units)
}

// JS parseInt rounds to a Number, then fromCharCode applies ToUint16. The
// remainder is a nonnegative integer smaller than 65536, so this cast is bounded.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn entity_code_unit(digits: &str) -> u16 {
    let number = digits.parse::<f64>().unwrap_or(f64::INFINITY);
    if number.is_finite() {
        (number % 65536.0) as u16
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn parameter_depth_limits_and_decoding_are_explicit() {
        assert!(parse("a=x&".repeat(1000).as_bytes()).is_err());
        assert!(parse("a=x&".repeat(999).as_bytes()).is_ok());
        assert!(parse(format!("a{}=x", "[b]".repeat(33)).as_bytes()).is_err());
        assert_eq!(
            parse(b"a[b]=x&a[c][]=y&a[c][]=z"),
            Ok(json!({"a":{"b":"x","c":["y","z"]}}))
        );
        assert_eq!(parse(&[255]).unwrap(), serde_json::json!({"�":""}));
        assert_eq!(
            parse(b"__proto__=x&constructor=y&=ignored"),
            Ok(json!({"constructor":"y"}))
        );
        assert_eq!(
            parse(b"a=%ff+%20&b=%E6%B0%B4&c=%2"),
            Ok(json!({"a":"%ff %20","b":"水","c":"%2"}))
        );
        assert_eq!(
            parse(b"a=1&a[]=2&a%5B%5D=3"),
            Ok(json!({"a":["1","2","3"]}))
        );
    }
}
