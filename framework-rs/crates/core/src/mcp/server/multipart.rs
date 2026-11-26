//! JSON MCP arguments become multipart text fields, matching TypeScript `FormData`.
use serde_json::{Map, Value};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn encode(value: &Value) -> Result<(Vec<u8>, String), String> {
    let fields = value
        .as_object()
        .ok_or("Multipart body must be an object of strings")?;
    if fields.values().any(|value| !value.is_string()) {
        return Err("Multipart body must be an object of strings".into());
    }
    // Check the selected delimiter against every field. A caller-controlled
    // value can never terminate a part or insert an additional MIME header.
    for _ in 0..8 {
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let boundary = format!(
            "forklaunch-mcp-{time:x}-{:x}",
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        if let Ok(body) = serialize(fields, &boundary) {
            return Ok((body, boundary));
        }
    }
    Err("Unable to select multipart boundary".into())
}
fn normalize(value: &str) -> String {
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\r\n")
}
fn serialize(fields: &Map<String, Value>, boundary: &str) -> Result<Vec<u8>, String> {
    if fields.iter().any(|(key, value)| {
        key.contains(boundary) || value.as_str().is_some_and(|value| value.contains(boundary))
    }) {
        return Err("Multipart boundary collision".into());
    }
    let mut output = String::new();
    for (name, value) in fields {
        let name = normalize(name)
            .replace('\r', "%0D")
            .replace('\n', "%0A")
            .replace('"', "%22");
        let value = value
            .as_str()
            .ok_or("Multipart body must be an object of strings")?;
        output.push_str("--");
        output.push_str(boundary);
        output.push_str("\r\nContent-Disposition: form-data; name=\"");
        output.push_str(&name);
        output.push_str("\"\r\n\r\n");
        output.push_str(&normalize(value));
        output.push_str("\r\n");
    }
    output.push_str("--");
    output.push_str(boundary);
    output.push_str("--\r\n");
    Ok(output.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn formdata_encoding_preserves_unicode_and_escapes_names_without_header_injection() {
        let fields = json!({"a\"\r\nX-Test: value":"a\nb\rc\r\nd 水","empty":""});
        let result =
            String::from_utf8(serialize(fields.as_object().unwrap(), "fixture-boundary").unwrap())
                .unwrap();
        assert_eq!(
            result,
            "--fixture-boundary\r\nContent-Disposition: form-data; name=\"a%22%0D%0AX-Test: value\"\r\n\r\na\r\nb\r\nc\r\nd 水\r\n--fixture-boundary\r\nContent-Disposition: form-data; name=\"empty\"\r\n\r\n\r\n--fixture-boundary--\r\n"
        );
        assert!(
            serialize(
                json!({"content":"--fixture-boundary"}).as_object().unwrap(),
                "fixture-boundary"
            )
            .is_err()
        );
        for input in [
            json!([]),
            json!(null),
            json!({"flag":false}),
            json!({"values":["a"]}),
            json!({"value":0}),
            json!({"file":{"base64":"AA=="}}),
        ] {
            assert!(encode(&input).is_err());
        }
    }
    #[test]
    fn boundaries_are_distinct_and_an_empty_form_is_well_formed() {
        let (first, a) = encode(&json!({})).unwrap();
        let (_, b) = encode(&json!({})).unwrap();
        assert_ne!(a, b);
        assert_eq!(first, format!("--{a}--\r\n").as_bytes());
    }
}
