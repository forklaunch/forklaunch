use forklaunch_universal_sdk::{
    MatchedVersionResponses, SdkError, decode_json_response, decode_text_response,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{Read as _, Write as _};

async fn response(status: u16, media: &str, body: &str) -> reqwest::Response {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/value?edition=old",
        listener.local_addr().unwrap()
    );
    let wire = format!(
        "HTTP/1.1 {status} Test\r\nContent-Type: {media}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        let _ = socket.read(&mut request).unwrap();
        socket.write_all(wire.as_bytes()).unwrap();
    });
    let result = reqwest::get(url).await.unwrap();
    server.join().unwrap();
    result
}

async fn matched(schema: Value, media: &str) -> MatchedVersionResponses {
    let branch = |edition: &str, schema: Value| {
        json!({
            "path":"/value", "params":{}, "headers":{},
            "query":{"edition":{"required":true,"schema":{"const":edition}}},
            "bodyKind":"empty", "bodyOptional":false,
            "responses":{format!("200:{media}"):{"schema":schema,"headers":{}}}
        })
    };
    let metadata = json!({"formatVersion":1,"branches":[
        branch("old",schema), branch("new",true.into())
    ]})
    .to_string();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        let _ = socket.read(&mut request).unwrap();
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .unwrap();
    });
    let request = forklaunch_universal_sdk::SdkRequest {
        method: "GET",
        url: format!("{base}/value"),
        query: [("edition".into(), "old".into())].into(),
        headers: std::collections::BTreeMap::default(),
        body: None,
        body_bytes: None,
        multipart: None,
        content_type: "",
    };
    let sdk =
        forklaunch_universal_sdk::UniversalSdk::<forklaunch_universal_sdk::NoMultipart>::new(base)
            .with_plaintext_development_transport();
    let (_, matched) = sdk.execute_versioned(request, &metadata).await.unwrap();
    server.join().unwrap();
    matched
}

#[derive(Debug, Deserialize, PartialEq)]
struct Count {
    count: u64,
}
const COUNT: &str = r#"{"type":"object","required":["count"],"additionalProperties":false,"properties":{"count":{"type":"integer","minimum":0,"maximum":10}}}"#;

#[tokio::test]
async fn json_helper_matches_nullable_generated_models_and_rejects_invalid_values() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct Label {
        label: Option<String>,
    }
    let schema = r#"{"type":"object","required":["label"],"properties":{"label":{"type":"string","nullable":true,"minLength":2}}}"#;
    for (body, label) in [
        (r#"{"label":null}"#, None),
        (r#"{"label":"valid"}"#, Some("valid".into())),
    ] {
        assert_eq!(
            decode_json_response::<Label>(
                response(200, "application/json", body).await,
                schema,
                None
            )
            .await
            .unwrap(),
            Label { label }
        );
    }
    for body in [r#"{"label":7}"#, r#"{"label":"x"}"#, "{}"] {
        assert!(matches!(
            decode_json_response::<Label>(
                response(200, "application/json", body).await,
                schema,
                None
            )
            .await,
            Err(SdkError::Contract("response violates contract"))
        ));
    }
}

#[tokio::test]
async fn json_helper_validates_before_typed_deserialization() {
    let good = response(200, "application/json", r#"{"count":3}"#).await;
    assert_eq!(
        decode_json_response::<Count>(good, COUNT, None)
            .await
            .unwrap(),
        Count { count: 3 }
    );
    for body in [
        r#"{"count":-1}"#,
        r#"{"count":11}"#,
        r#"{"count":"3"}"#,
        "{}",
    ] {
        let wire = response(200, "application/json", body).await;
        assert!(matches!(
            decode_json_response::<Count>(wire, COUNT, None).await,
            Err(SdkError::Contract("response violates contract"))
        ));
    }
    let malformed = response(200, "application/json", "{").await;
    assert!(matches!(
        decode_json_response::<Count>(malformed, COUNT, None).await,
        Err(SdkError::Contract("invalid response JSON"))
    ));
    let wrong_type = response(200, "application/json", "1").await;
    assert!(matches!(
        decode_json_response::<Count>(wrong_type, "true", None).await,
        Err(SdkError::Contract("invalid typed response"))
    ));
}

#[tokio::test]
async fn json_helper_keeps_declared_and_matched_version_checks() {
    let versions = matched(
        json!({"type":"object","required":["count"],"properties":{"count":{"const":3}}}),
        "application/json",
    )
    .await;
    for (status, media, body) in [
        (200, "application/json", r#"{"count":4}"#),
        (201, "application/json", r#"{"count":3}"#),
        (200, "text/plain", r#"{"count":3}"#),
    ] {
        assert!(
            decode_json_response::<Count>(
                response(status, media, body).await,
                COUNT,
                Some(&versions)
            )
            .await
            .is_err()
        );
    }
    assert!(
        decode_json_response::<Count>(
            response(200, "application/json", r#"{"count":3}"#).await,
            COUNT,
            Some(&versions)
        )
        .await
        .is_ok()
    );
    let strict = r#"{"type":"object","properties":{"count":{"const":4}}}"#;
    assert!(
        decode_json_response::<Count>(
            response(200, "application/json", r#"{"count":3}"#).await,
            strict,
            Some(&versions)
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn text_helper_keeps_schema_versions_and_media_ambiguity_checks() {
    let schema = r#"{"type":"string","minLength":3}"#;
    assert_eq!(
        decode_text_response(response(200, "text/plain", "valid").await, schema, None)
            .await
            .unwrap(),
        "valid"
    );
    assert!(
        decode_text_response(response(200, "text/plain", "x").await, schema, None)
            .await
            .is_err()
    );
    let versions = matched(json!({"const":"old"}), "text/plain").await;
    assert!(
        decode_text_response(
            response(200, "text/plain", "old").await,
            schema,
            Some(&versions)
        )
        .await
        .is_ok()
    );
    assert!(
        decode_text_response(
            response(200, "text/plain", "new").await,
            schema,
            Some(&versions)
        )
        .await
        .is_err()
    );
    assert!(
        decode_text_response(
            response(200, "text/html", "old").await,
            schema,
            Some(&versions)
        )
        .await
        .is_err()
    );
    assert!(
        decode_text_response(
            response(200, "text/plain", "old").await,
            r#"{"const":"other"}"#,
            Some(&versions)
        )
        .await
        .is_err()
    );
    let duplicate = response(200, "text/plain\r\nContent-Type: text/html", "old").await;
    assert!(matches!(
        decode_text_response(duplicate, schema, None).await,
        Err(SdkError::Contract("ambiguous response content type"))
    ));
}
