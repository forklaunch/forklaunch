use super::*;
use crate::ForkLaunchRouter;
use axum::{
    Extension, Json,
    body::{Body, to_bytes},
    extract::DefaultBodyLimit,
};
use forklaunch_core::contract::{BodyContract, BodyKind, RouteContract, SchemaRef};
use serde::{Deserialize, Serialize};
use tower::ServiceExt;

#[derive(Deserialize, Serialize, schemars::JsonSchema, garde::Validate)]
#[serde(deny_unknown_fields)]
struct Upload {
    #[garde(length(min = 1))]
    label: String,
    #[garde(skip)]
    file: Vec<u8>,
}

fn app(limits: MultipartLimits) -> axum::Router {
    let contract = RouteContract::post("Upload", "Upload", "/upload")
        .with_request_body(BodyContract {
            kind: BodyKind::Multipart,
            schema: SchemaRef::named("Upload"),
            content_type: Some("multipart/form-data".into()),
        })
        .with_response(200, None)
        .with_response(400, None)
        .with_response(413, None);
    ForkLaunchRouter::anonymous()
        .post(contract, |Multipart(value): Multipart<Upload>| async move {
            Json(value)
        })
        .into_router()
        .layer(Extension(limits))
}
fn wire(parts: &[(&str, Option<&str>, &[u8])]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (name, filename, content) in parts {
        bytes.extend_from_slice(
            format!("--boundary\r\nContent-Disposition: form-data; name=\"{name}\"").as_bytes(),
        );
        if let Some(filename) = filename {
            bytes.extend_from_slice(format!("; filename=\"{filename}\"").as_bytes());
        }
        bytes.extend_from_slice(b"\r\n\r\n");
        bytes.extend_from_slice(content);
        bytes.extend_from_slice(b"\r\n");
    }
    bytes.extend_from_slice(b"--boundary--\r\n");
    bytes
}
async fn send(app: axum::Router, bytes: Vec<u8>, mime: &str) -> Response {
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri("/upload")
            .header("content-type", mime)
            .body(Body::from(bytes))
            .unwrap(),
    )
    .await
    .unwrap()
}
const MIME: &str = "multipart/form-data; boundary=\"boundary\"";

#[tokio::test]
async fn extracts_validated_text_and_binary_with_last_part_winning() {
    let data = wire(&[
        ("label", None, b"first"),
        ("file", Some("../ignored.bin"), &[0, 255, 128]),
        ("label", None, "水".as_bytes()),
    ]);
    let response = send(app(MultipartLimits::default()), data, MIME).await;
    assert_eq!(response.status(), StatusCode::OK);
    let value: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024).await.unwrap()).unwrap();
    assert_eq!(value, serde_json::json!({"label":"水", "file":[0,255,128]}));
    let response = send(
        app(MultipartLimits::default()),
        wire(&[("label", None, b"ok"), ("file", Some(""), b"")]),
        MIME,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn rejects_malformed_missing_unknown_invalid_and_truncated_inputs() {
    let cases = [
        wire(&[("file", Some("a"), b"secret")]),
        wire(&[("label", None, b""), ("file", Some("a"), b"secret")]),
        wire(&[("label", None, b"secret"), ("file", None, b"not-bytes")]),
        wire(&[("label", None, &[255]), ("file", Some("a"), b"secret")]),
        wire(&[
            ("label", None, b"secret"),
            ("file", Some("a"), b""),
            ("extra", None, b"secret"),
        ]),
        b"--boundary\r\nContent-Disposition: form-data; name=\"label\"\r\n\r\nsecret".to_vec(),
    ];
    for data in cases {
        let response = send(app(MultipartLimits::default()), data, MIME).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(
            !String::from_utf8(to_bytes(response.into_body(), 1024).await.unwrap().to_vec())
                .unwrap()
                .contains("secret")
        );
    }
    let response = send(
        app(MultipartLimits::default()),
        vec![],
        "multipart/form-data",
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn bounds_fields_total_and_repeated_parts() {
    let data = wire(&[("label", None, b"ok"), ("file", Some("a"), b"abc")]);
    for limits in [
        MultipartLimits {
            parts: 1,
            ..MultipartLimits::default()
        },
        MultipartLimits {
            field_bytes: 2,
            ..MultipartLimits::default()
        },
        MultipartLimits {
            total_bytes: 4,
            ..MultipartLimits::default()
        },
    ] {
        assert_eq!(
            send(app(limits), data.clone(), MIME).await.status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }
    let exact = MultipartLimits {
        parts: 2,
        field_bytes: 3,
        total_bytes: 5,
    };
    assert_eq!(
        send(app(exact), data.clone(), MIME).await.status(),
        StatusCode::OK
    );
    let duplicate = wire(&[
        ("label", None, b"ok"),
        ("label", None, b"ok"),
        ("file", Some("a"), b"abc"),
    ]);
    assert_eq!(
        send(app(exact), duplicate, MIME).await.status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(
        send(
            app(MultipartLimits::default()).layer(DefaultBodyLimit::max(20)),
            data,
            MIME
        )
        .await
        .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
}

#[tokio::test]
async fn multipart_matches_typescript_parser() {
    let Ok(path) = std::env::var("FORKLAUNCH_TEST_MULTIPART_CASES") else {
        return;
    };
    let cases: Vec<Value> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    for (index, case) in cases.into_iter().enumerate() {
        let bytes: Vec<u8> = serde_json::from_value(case["bytes"].clone()).unwrap();
        let response = send(app(MultipartLimits::default()), bytes, MIME).await;
        assert_eq!(response.status(), StatusCode::OK, "case {index}");
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
        assert_eq!(body, case["expected"], "case {index}");
    }
}

#[tokio::test]
async fn busboy_text_profile_matches_actual_parser_with_binary_preserved() {
    let Ok(path) = std::env::var("FORKLAUNCH_MULTIPART_REFERENCE_OUTPUT") else {
        return;
    };
    let cases: Vec<Value> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut compared = 0;
    for case in cases {
        if case.get("collision").is_some() {
            continue;
        }
        let bytes: Vec<u8> = serde_json::from_value(case["bytes"].clone()).unwrap();
        let charset = case["charset"].as_str().unwrap();
        let mime = if charset == "default" {
            String::new()
        } else {
            format!("Content-Type: text/plain; charset={charset}\r\n")
        };
        let mut data =
            format!("--boundary\r\nContent-Disposition: form-data; name=\"label\"\r\n{mime}\r\n")
                .into_bytes();
        data.extend_from_slice(&bytes);
        data.extend_from_slice(b"\r\n--boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"ignored\"\r\nContent-Type: text/plain; charset=unknown-owned\r\n\r\n\0\xff\r\n--boundary--\r\n");
        let response = send(
            app(MultipartLimits::default()).layer(Extension(BusboyTextOptions::default())),
            data,
            MIME,
        )
        .await;
        if case["undefined"] == true || case["value"] == "" {
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{case}");
        } else {
            assert_eq!(response.status(), StatusCode::OK, "{case}");
            let body: Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap())
                    .unwrap();
            assert_eq!(
                body,
                serde_json::json!({"label":case["value"],"file":[0,255]}),
                "{case}"
            );
        }
        compared += 1;
    }
    assert_eq!(compared, 45);
}
