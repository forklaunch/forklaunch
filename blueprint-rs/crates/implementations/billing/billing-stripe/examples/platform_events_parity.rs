//! Actual TypeScript/native platform-event verifier conformance driver.
use forklaunch_core::auth::request_hmac::generate_hmac_auth_header;
use forklaunch_core::http::platform_events::{
    MemoryPlatformEventNonces, PlatformEventNonceStore, PlatformEventVerifier,
};
use serde_json::{Value, json};
use std::{
    io::{self, Read},
    sync::Arc,
};
#[tokio::main]
async fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let input: Value = serde_json::from_str(&input).unwrap();
    let secret = "instance-secret-fixture";
    let verifier = PlatformEventVerifier::new(
        secret.into(),
        Arc::new(MemoryPlatformEventNonces::default()),
    )
    .unwrap();
    let mut results = Vec::new();
    for action in input["actions"].as_array().unwrap() {
        let result = verifier
            .verify_at(
                action["method"].as_str().unwrap(),
                action["path"].as_str().unwrap(),
                action["authorization"].as_str().unwrap(),
                action["body"].as_str().unwrap().as_bytes(),
                action["now"].as_i64().unwrap(),
            )
            .await;
        results.push(match result {
            Ok(event) => json!({"ok":event.into_payload()}),
            Err(error) => json!({"error":true,"code":error.code,"description":error.to_string()}),
        });
    }
    let store = Arc::new(MemoryPlatformEventNonces::new(1).unwrap());
    assert!(store.claim("occupied", 100, 600).await.unwrap());
    assert!(store.claim("new", 600, 700).await.is_err());
    assert!(store.claim("new", 601, 700).await.unwrap());
    assert!(MemoryPlatformEventNonces::new(0).is_err());
    assert!(
        PlatformEventVerifier::new(
            String::new(),
            Arc::new(MemoryPlatformEventNonces::default())
        )
        .is_err()
    );
    let now = input["now"].as_i64().unwrap();
    let timestamp = chrono::DateTime::from_timestamp_millis(now)
        .unwrap()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let body = r#"{"id":"concurrent","type":"email.delivered","feature":"email"}"#;
    let authorization = generate_hmac_auth_header(
        secret.as_bytes(),
        "platform",
        "POST",
        "/platform-events/email",
        forklaunch_core::auth::request_hmac::SignedBody::Text(body),
        &timestamp,
        "concurrent-delivery",
    )
    .unwrap();
    let mut tasks = Vec::new();
    for _ in 0..100 {
        let verifier = verifier.clone();
        let authorization = authorization.clone();
        tasks.push(tokio::spawn(async move {
            verifier
                .verify_at(
                    "POST",
                    "/platform-events/email",
                    &authorization,
                    body.as_bytes(),
                    now,
                )
                .await
                .is_ok()
        }));
    }
    let mut winners = 0;
    for task in tasks {
        winners += usize::from(task.await.unwrap());
    }
    assert_eq!(winners, 1);
    assert!(!format!("{verifier:?}").contains(secret));
    println!(
        "{}",
        json!({"results":results,"concurrentWinners":winners,"boundedInclusiveExpiry":true,"redactedConfiguration":true})
    );
}
