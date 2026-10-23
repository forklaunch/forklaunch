use forklaunch_universal_sdk::{
    MultipartForm, ResponseValidator, SdkError, SdkEventStream, SdkRequest, UniversalSdk,
    decode_response_headers,
};
use std::collections::BTreeMap;

fn request(url: String) -> SdkRequest {
    SdkRequest {
        method: "GET",
        url,
        query: BTreeMap::new(),
        headers: BTreeMap::new(),
        body: None,
        body_bytes: None,
        multipart: None,
        content_type: "application/json",
    }
}

#[test]
fn nonmultipart_requests_keep_clone_equality_and_debug_redacts_values() {
    let mut input = request("https://example.invalid/private-canary".into());
    input
        .headers
        .insert("authorization".into(), "private-canary".into());
    input.body = Some("private-canary".into());
    assert_eq!(input, input.clone());
    let multipart = UniversalSdk::<MultipartForm>::new("https://example.invalid");
    let multipart = multipart.clone();
    assert!(!format!("{input:?} {multipart:?}").contains("private-canary"));
    let client = UniversalSdk::<forklaunch_universal_sdk::NoMultipart>::new(
        "https://private-canary.invalid",
    )
    .with_bearer_token("private-canary");
    assert!(!format!("{client:?}").contains("private-canary"));
}

#[tokio::test]
async fn hook_cannot_bypass_transport_admission() {
    let client = UniversalSdk::new("https://example.invalid").with_auth_hook(|input| {
        input.url = "http://user:secret@example.invalid/private-canary".into();
    });
    assert!(matches!(
        client
            .execute(request("https://example.invalid".into()))
            .await,
        Err(SdkError::TransportPolicy)
    ));
    for url in [
        "https://user:secret@example.invalid",
        "https://example.invalid/#fragment",
        "file:///private",
    ] {
        assert!(matches!(
            UniversalSdk::new(url).execute(request(url.into())).await,
            Err(SdkError::TransportPolicy)
        ));
    }
}

#[test]
fn response_validation_and_duplicate_headers_fail_closed() {
    let validator = ResponseValidator::new(r#"{"type":"integer","minimum":3}"#).unwrap();
    assert!(validator.validate(&serde_json::json!(4)).is_ok());
    assert!(validator.validate(&serde_json::json!(2)).is_err());
    assert!(ResponseValidator::new(r#"{"$ref":"https://example.invalid/schema"}"#).is_err());
    let mut headers = reqwest::header::HeaderMap::new();
    headers.append("x-count", "3".parse().unwrap());
    let rules = r#"{"x-count":{"required":true,"schema":{"type":"integer"}}}"#;
    let decoded: serde_json::Value = decode_response_headers(&headers, rules).unwrap();
    assert_eq!(decoded["x-count"], 3);
    headers.append("x-count", "4".parse().unwrap());
    assert!(decode_response_headers::<serde_json::Value>(&headers, rules).is_err());
}

#[tokio::test]
async fn real_transport_preserves_auth_and_checked_stream_closes_on_invalid_event() {
    use axum::{Router, http::HeaderMap, routing::get};
    let app = Router::new().route(
        "/events",
        get(|headers: HeaderMap| async move {
            assert_eq!(headers["authorization"], "Bearer test-token");
            assert_eq!(headers["x-version"], "request");
            (
                [("content-type", "text/event-stream")],
                "data: {\"n\":1}\n\ndata: {\"n\":\"private-canary\"}\n\n",
            )
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = UniversalSdk::new(format!("http://{address}"))
        .with_bearer_token("test-token")
        .with_default_header("x-version", "default");
    let mut input = request(format!("http://{address}/events"));
    input.headers.insert("x-version".into(), "request".into());
    let response = client.execute(input).await.unwrap();
    let mut stream = SdkEventStream::<serde_json::Value>::from_response(response, 1024,
        r#"{"type":"object","properties":{"data":{"type":"object","properties":{"n":{"type":"integer"}},"required":["n"]}},"required":["data"]}"#).unwrap();
    assert_eq!(stream.next().await.unwrap().unwrap()["data"]["n"], 1);
    let error = stream.next().await.unwrap_err();
    assert!(!format!("{error:?} {stream:?}").contains("private-canary"));
    assert!(stream.next().await.unwrap().is_none());
    server.abort();
}

#[tokio::test]
async fn multipart_preserves_empty_boundary_and_enforces_response_bound() {
    use axum::{
        Router,
        body::Bytes,
        http::HeaderMap,
        routing::{get, post},
    };
    let app = Router::new()
        .route("/empty", post(|headers: HeaderMap, bytes: Bytes| async move {
            let media = headers["content-type"].to_str().unwrap();
            let boundary = media.split("boundary=").nth(1).unwrap();
            assert_eq!(bytes.as_ref(), format!("--{boundary}--\r\n").as_bytes());
            "accepted"
        }))
        .route("/form", get(|| async {
            ([("content-type", "multipart/form-data; boundary=fixture")],
                "--fixture\r\nContent-Disposition: form-data; name=title\r\n\r\nhello\r\n--fixture--\r\n")
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let input = SdkRequest {
        method: "POST",
        url: format!("http://{address}/empty"),
        query: BTreeMap::new(),
        headers: BTreeMap::from([("content-type".into(), "incorrect-caller-boundary".into())]),
        body: None,
        body_bytes: None,
        multipart: Some(MultipartForm::new()),
        content_type: "multipart/form-data",
    };
    let client = UniversalSdk::<MultipartForm>::new(format!("http://{address}"));
    assert_eq!(
        client.execute(input).await.unwrap().text().await.unwrap(),
        "accepted"
    );
    let client = UniversalSdk::new(format!("http://{address}"));
    let response = client
        .execute(request(format!("http://{address}/form")))
        .await
        .unwrap();
    let form = forklaunch_universal_sdk::decode_multipart_response(response, 1024)
        .await
        .unwrap();
    assert_eq!(form.parts[0].bytes, b"hello");
    let response = client
        .execute(request(format!("http://{address}/form")))
        .await
        .unwrap();
    assert!(
        forklaunch_universal_sdk::decode_multipart_response(response, 2)
            .await
            .is_err()
    );
    server.abort();
}
