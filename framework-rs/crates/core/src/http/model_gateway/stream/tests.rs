use super::*;
use serde_json::json;
#[test]
fn decoder_matches_actual_data_lines_tail_and_utf8_rules() {
    for (bytes, initial, tail, expected) in [
        (
            b"data: {\"id\":\ndata: \"x\"}".as_slice(),
            true,
            false,
            Some(json!({"id":"x"})),
        ),
        (b"data: {\"id\":\ndata: \"x\"}".as_slice(), true, true, None),
        (
            b"data: \"ok\"\xe2\x82".as_slice(),
            true,
            true,
            Some(json!("ok")),
        ),
        (
            b"data: \"\xff\"".as_slice(),
            true,
            false,
            Some(json!("\u{fffd}")),
        ),
        (
            b"\xef\xbb\xbfdata: null".as_slice(),
            true,
            false,
            Some(Value::Null),
        ),
        (b"\xef\xbb\xbfdata: null".as_slice(), false, false, None),
        ("data: \u{0085}null".as_bytes(), true, false, None),
        (
            "data: \u{00a0}null".as_bytes(),
            true,
            false,
            Some(Value::Null),
        ),
        (b"data: [DONE] ".as_slice(), true, false, None),
    ] {
        let got = match decode_event(bytes, initial, tail) {
            Event::Value(value) => Some(value),
            Event::Skip => None,
            Event::Done => panic!("unexpected DONE"),
        };
        assert_eq!(got, expected);
    }
    assert!(matches!(
        decode_event(b"data: [DONE]", true, false),
        Event::Done
    ));
    assert!(matches!(
        decode_event(b"data: [DONE]\r", true, false),
        Event::Skip
    ));
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
        ("data: 9007199254740993", 0x4340_0000_0000_0000_u64),
        ("data: -0", 0x8000_0000_0000_0000_u64),
    ] {
        let Event::Value(Value::Number(number)) = decode_event(text.as_bytes(), true, false) else {
            panic!("numeric event")
        };
        assert!(number.is_f64());
        assert_eq!(number.as_f64().unwrap().to_bits(), bits);
    }
}
