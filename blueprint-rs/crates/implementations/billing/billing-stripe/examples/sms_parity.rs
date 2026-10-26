//! Actual TypeScript/native managed SMS conformance driver.
use forklaunch_core::http::instance_gateway::InstanceGatewayClient;
use forklaunch_core::http::sms_client::SmsClient;
use forklaunch_core::http::sms_client::SmsSendRequest;
use forklaunch_core::http::sms_client::is_e164;
use forklaunch_core::http::sms_client::sms_segments;
use serde_json::{Value, json};
use std::io::{self, Read};
#[tokio::main]
async fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let input: Value = serde_json::from_str(&input).unwrap();
    let client = SmsClient::new(
        InstanceGatewayClient::new(
            input["base"].as_str().unwrap(),
            "instance_fixture".into(),
            "instance-secret-fixture".into(),
        )
        .unwrap(),
    );
    let mut results = Vec::new();
    for action in input["actions"].as_array().unwrap() {
        if action["method"] == "segments" {
            results.push(json!({"ok":sms_segments(action["body"].as_str().unwrap())}));
            continue;
        }
        if action["method"] == "phone" {
            results.push(json!({"ok":is_e164(action["to"].as_str().unwrap())}));
            continue;
        }
        let Ok(request) = serde_json::from_value::<SmsSendRequest>(action["request"].clone())
        else {
            results.push(json!({"error":true,"status":null,"retryAfterSeconds":null}));
            continue;
        };
        let result = if action["method"] == "deferred" {
            let deferred = SmsClient::deferred();
            assert!(!deferred.managed());
            deferred.send(&request).await
        } else {
            assert!(client.managed());
            client.send(&request).await
        };
        results.push(match result{Ok(value)=>json!({"ok":value}),Err(error)=>json!({"error":true,"status":error.status,"retryAfterSeconds":error.retry_after_seconds,"description":error.to_string()})});
    }
    println!("{}", json!(results));
}
