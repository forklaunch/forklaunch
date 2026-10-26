use super::*;

#[test]
fn every_chunk_boundary_preserves_utf8_crlf_metadata_and_multiline_data() {
    let source = "\u{feff}: heartbeat\r\nid: first\r\nevent: update\r\nretry: 1500\r\ndata: {\r\ndata:  \"label\": \"水 🦀\"\r\ndata: }\r\n\r\ndata: second\n\nid: \rdata:\r\rid: ignored\0value\ndata:  padded \n\n";
    let expected = vec![
        SseEvent {
            id: Some("first".into()),
            event: Some("update".into()),
            retry_ms: Some(1500),
            data: "{\n \"label\": \"水 🦀\"\n}".into(),
        },
        SseEvent {
            id: Some("first".into()),
            data: "second".into(),
            ..Default::default()
        },
        SseEvent {
            id: Some(String::new()),
            data: String::new(),
            ..Default::default()
        },
        SseEvent {
            id: Some(String::new()),
            data: " padded ".into(),
            ..Default::default()
        },
    ];
    for size in 1..=source.len() {
        let mut decoder = SseDecoder::new(1024).unwrap();
        let mut actual = Vec::new();
        for bytes in source.as_bytes().chunks(size) {
            actual.extend(decoder.push(bytes).unwrap());
        }
        assert_eq!(actual, expected, "chunk size {size}");
        assert_eq!(decoder.retry_ms(), Some(1500));
        decoder.finish();
        assert_eq!(decoder.push(b"data: late\n\n"), Err(SseError::Closed));
    }
}

#[test]
fn codecs_roundtrip_empty_unicode_and_multiline_events_without_metadata_injection() {
    for data in ["", "水 🦀", " leading ", "one\ntwo\n", "one\r\ntwo\rthree"] {
        let event = SseEvent {
            data: data.into(),
            id: Some("id-1".into()),
            event: Some("update".into()),
            retry_ms: Some(100),
        };
        let encoded = event.encode().unwrap();
        let mut decoder = SseDecoder::new(4096).unwrap();
        let mut expected = event;
        expected.data = data.replace("\r\n", "\n").replace('\r', "\n");
        assert_eq!(decoder.push(&encoded).unwrap(), [expected]);
    }
    for metadata in ["bad\nid: injected", "bad\rdata: injected", "bad\0"] {
        assert_eq!(
            SseEvent {
                id: Some(metadata.into()),
                ..Default::default()
            }
            .encode(),
            Err(SseError::InvalidMetadata)
        );
        assert_eq!(
            SseEvent {
                event: Some(metadata.into()),
                ..Default::default()
            }
            .encode(),
            Err(SseError::InvalidMetadata)
        );
    }
}

#[test]
fn limits_invalid_utf8_retry_fields_and_end_of_stream_are_explicit() {
    assert!(matches!(SseDecoder::new(0), Err(SseError::InvalidLimit)));
    let mut decoder = SseDecoder::new(12).unwrap();
    assert_eq!(decoder.push(b"data: first\n"), Ok(vec![]));
    assert_eq!(decoder.push(b"data: second"), Err(SseError::EventTooLarge));
    assert_eq!(decoder.push(b"\n\n"), Err(SseError::Closed));
    let mut decoder = SseDecoder::new(64).unwrap();
    for _ in 0..10 {
        assert_eq!(decoder.push(b"data: ok\n\n").unwrap().len(), 1);
    }
    assert!(
        decoder
            .push(b"retry: -1\nretry: 1.5\nretry: 18446744073709551616\n\n")
            .unwrap()
            .is_empty()
    );
    assert_eq!(decoder.retry_ms(), None);
    assert_eq!(decoder.push(b"data: \xff\n\n").unwrap()[0].data, "\u{fffd}");
    assert!(decoder.push(b"data: incomplete").unwrap().is_empty());
    decoder.finish();
    assert!(decoder.data.is_empty());
    assert!(decoder.line.is_empty());
}

#[test]
fn native_sse_transport_conformance() {
    let Ok(input) = std::env::var("FORKLAUNCH_TEST_SSE_INPUT") else {
        return;
    };
    let cases: Vec<Vec<Vec<u8>>> = serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
    let results: Vec<Vec<serde_json::Value>> = cases
        .into_iter()
        .map(|chunks| {
            let mut decoder = SseDecoder::new(1024 * 1024).unwrap();
            let mut events = Vec::new();
            for chunk in chunks {
                for event in decoder.push(&chunk).unwrap() {
                    let data = serde_json::from_str(&event.data)
                        .unwrap_or(serde_json::Value::String(event.data));
                    let mut value = serde_json::json!({"data": data});
                    if let Some(id) = event.id {
                        value["id"] = serde_json::Value::String(id);
                    }
                    events.push(value);
                }
            }
            decoder.finish();
            events
        })
        .collect();
    std::fs::write(
        std::env::var("FORKLAUNCH_TEST_SSE_OUTPUT").unwrap(),
        serde_json::to_vec(&results).unwrap(),
    )
    .unwrap();
}

#[test]
fn complete_events_preceding_overflow_are_delivered_before_the_terminal_failure() {
    let mut decoder = SseDecoder::new(16).unwrap();
    let events = decoder
        .push(b"data: first\n\ndata: this event is too large\n\n")
        .unwrap();
    assert_eq!(events, [SseEvent::new("first")]);
    assert_eq!(decoder.failure(), Some(SseError::EventTooLarge));
    assert_eq!(decoder.push(b"data: later\n\n"), Err(SseError::Closed));
}
