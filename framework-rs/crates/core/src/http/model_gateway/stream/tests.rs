use super::*;
use serde_json::json;
fn decoded(bytes: &[u8], initial: bool) -> &'static str {
    match decode_event(bytes, initial) {
        Event::Value(_) => "value",
        Event::Skip => "skip",
        Event::Done => "done",
        Event::Invalid => "invalid",
    }
}
#[test]
fn decoder_accepts_only_chunks_and_exact_done() {
    let chunk = decode_event(b"data: {\"id\":\ndata: \"x\",\"choices\":[]}", true);
    let Event::Value(value) = chunk else {
        panic!("multiline chunk")
    };
    assert_eq!(value, json!({"id":"x","choices":[]}));
    for (bytes, initial, expected) in [
        (b"data: [DONE]".as_slice(), true, "done"),
        (b": comment".as_slice(), true, "skip"),
        (b"data:".as_slice(), true, "skip"),
        (
            b"\xef\xbb\xbfdata: {\"choices\":[]}".as_slice(),
            true,
            "value",
        ),
        // Only the stream's first frame may begin with a byte-order mark.
        (b"\xef\xbb\xbfdata: [DONE]".as_slice(), false, "skip"),
        ("data: \u{00a0}{\"choices\":[]}".as_bytes(), true, "value"),
        ("data: \u{0085}{\"choices\":[]}".as_bytes(), true, "invalid"),
        (b"data: {\"choices\":[\"\xff\"]}".as_slice(), true, "value"),
        (b"data: [DONE] ".as_slice(), true, "invalid"),
        (b"data: [DONE]\r".as_slice(), true, "invalid"),
        (b"data: bad".as_slice(), true, "invalid"),
        (b"data: null".as_slice(), true, "invalid"),
        (b"data: 0".as_slice(), true, "invalid"),
        (b"data: [1]".as_slice(), true, "invalid"),
        (b"data: {\"ok\":true}".as_slice(), true, "invalid"),
        (b"data: {\"choices\":{}}".as_slice(), true, "invalid"),
        (
            b"data: {\"choices\":[],\"error\":{}}".as_slice(),
            true,
            "invalid",
        ),
        (
            b"data: {\"choices\":[],\"error\":\"x\"}".as_slice(),
            true,
            "invalid",
        ),
        (
            b"data: {\"choices\":[],\"error\":1}".as_slice(),
            true,
            "invalid",
        ),
        (
            b"data: {\"choices\":[],\"error\":null}".as_slice(),
            true,
            "value",
        ),
        (
            b"data: {\"choices\":[],\"error\":false}".as_slice(),
            true,
            "value",
        ),
        (
            b"data: {\"choices\":[],\"error\":0}".as_slice(),
            true,
            "value",
        ),
        (
            b"data: {\"choices\":[],\"error\":-0}".as_slice(),
            true,
            "value",
        ),
        (
            b"data: {\"choices\":[],\"error\":\"\"}".as_slice(),
            true,
            "value",
        ),
    ] {
        assert_eq!(
            decoded(bytes, initial),
            expected,
            "{:?}",
            String::from_utf8_lossy(bytes)
        );
    }
}
#[test]
fn limits_reject_unbounded_profiles_and_delimiters_match_actual_regex() {
    for (total, frame, count, time) in [
        (0, 1, 1, 1),
        (1, 2, 1, 1),
        (1, 1, 0, 1),
        (1, 1, 1, 0),
        (1, 1, 1, 121),
        (16 * 1024 * 1024 + 1, 1, 1, 1),
        (1024 * 1024, 256 * 1024 + 1, 1, 1),
        (1, 1, 4097, 1),
    ] {
        assert!(ModelStreamLimits::new(total, frame, count, Duration::from_secs(time)).is_err());
    }
    for (bytes, size) in [
        (b"x\n\n".as_slice(), 2),
        (b"x\r\n\n".as_slice(), 3),
        (b"x\n\r\n".as_slice(), 3),
        (b"x\r\n\r\n".as_slice(), 4),
    ] {
        assert_eq!(boundary_end(bytes), Some(size));
    }
    assert_eq!(boundary_end(b"x\r\r"), None);
}

#[test]
fn streamed_numbers_use_browser_rounding_without_losing_negative_zero() {
    for (text, bits) in [
        (
            "data: {\"choices\":[9007199254740993]}",
            0x4340_0000_0000_0000_u64,
        ),
        ("data: {\"choices\":[-0]}", 0x8000_0000_0000_0000_u64),
    ] {
        let Event::Value(value) = decode_event(text.as_bytes(), true) else {
            panic!("numeric chunk")
        };
        let Value::Number(number) = &value["choices"][0] else {
            panic!("number")
        };
        assert!(number.is_f64());
        assert_eq!(number.as_f64().unwrap().to_bits(), bits);
    }
}
