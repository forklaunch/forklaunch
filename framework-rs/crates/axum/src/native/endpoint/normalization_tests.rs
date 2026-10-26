use super::*;
use forklaunch_core::contract::{HttpMethod, codec::Normalization};
use tower::ServiceExt;

#[forklaunch_core::contract::schema(unknown_fields = "deny")]
struct Input {
    #[field(wire_max_length=12,normalize=Normalization::TrimAscii,min_length=1,max_length=4)]
    title: String,
}
#[tokio::test]
async fn normalized_handler_values_use_distinct_raw_admission_schema() {
    let endpoint = Endpoint::<JsonBody<Input>, JsonBody<Input>>::new(
        "normalize",
        "Normalize",
        HttpMethod::Post,
        "/normalize",
        200,
    )
    .unwrap();
    let mut registry = ContractRegistry::new();
    registry.register(endpoint.contract());
    let app = ContractRouter::<()>::new(registry)
        .bind(endpoint, |_, input| async { Ok(input) })
        .unwrap()
        .finish()
        .unwrap();
    for (text, status, normalized) in [
        ("  one  ", 200, Some("one")),
        ("\t\none\r", 200, Some("one")),
        ("    x       ", 200, Some("x")),
        ("     x       ", 400, None),
        (" \t\n", 400, None),
        ("abcde", 400, None),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/normalize")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::json!({"title":text}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status);
        let bytes = to_bytes(response.into_body(), 10000).await.unwrap();
        if let Some(normalized) = normalized {
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
                serde_json::json!({"title":normalized})
            );
        }
    }
}

#[forklaunch_core::contract::schema(unknown_fields = "deny")]
struct DefaultInput {
    #[field(default, max_length = 4)]
    title: String,
    #[field(default, min = 0, max = 10)]
    count: i64,
    #[field(default)]
    active: bool,
}
#[tokio::test]
async fn omitted_members_reach_handler_as_validated_defaults() {
    let endpoint = Endpoint::<JsonBody<DefaultInput>, JsonBody<DefaultInput>>::new(
        "defaults",
        "Defaults",
        HttpMethod::Post,
        "/defaults",
        200,
    )
    .unwrap();
    let mut registry = ContractRegistry::new();
    registry.register(endpoint.contract());
    let app = ContractRouter::<()>::new(registry)
        .bind(endpoint, |_, input| async { Ok(input) })
        .unwrap()
        .finish()
        .unwrap();
    for (body, status) in [
        ("{}", 200),
        (r#"{"title":null}"#, 400),
        (r#"{"count":11}"#, 400),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/defaults")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status);
        if status == 200 {
            let bytes = to_bytes(response.into_body(), 10000).await.unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
                serde_json::json!({"title":"","count":0,"active":false})
            );
        }
    }
}

use forklaunch_core::contract::datetime::{DateTime, Utc};
#[forklaunch_core::contract::schema(unknown_fields = "deny")]
struct DateInput {
    date: DateTime<Utc>,
}
#[tokio::test]
async fn dates_reach_handler_as_native_values_and_emit_canonical_milliseconds() {
    let endpoint = Endpoint::<JsonBody<DateInput>, JsonBody<DateInput>>::new(
        "dates",
        "Dates",
        HttpMethod::Post,
        "/dates",
        200,
    )
    .unwrap();
    let mut registry = ContractRegistry::new();
    registry.register(endpoint.contract());
    let app = ContractRouter::<()>::new(registry)
        .bind(endpoint, |_, input| async {
            assert_eq!(input.date.timestamp(), 1_709_210_096);
            Ok(input)
        })
        .unwrap()
        .finish()
        .unwrap();
    for (date, status) in [
        ("2024-02-29T12:34:56Z", 200),
        ("2024-02-29T12:34:56.1Z", 200),
        ("2024-02-29T12:34:56.1234Z", 400),
        ("2024-02-29T12:34:60Z", 400),
        ("2023-02-29T12:34:56Z", 400),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/dates")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::json!({"date":date}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status);
        if status == 200 {
            let bytes = to_bytes(response.into_body(), 10000).await.unwrap();
            let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(json["date"].as_str().unwrap().len(), 24);
        }
    }
}
