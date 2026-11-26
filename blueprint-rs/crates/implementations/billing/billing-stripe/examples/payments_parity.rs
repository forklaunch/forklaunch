//! Native side of the actual TypeScript managed payments conformance fixture.
use forklaunch_core::http::instance_gateway::InstanceGatewayClient;
use forklaunch_core::http::payments_client::PaymentsClient;
use serde_json::{Value, json};
use std::io::{self, Read};
#[tokio::main]
async fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let input: Value = serde_json::from_str(&input).unwrap();
    let client = PaymentsClient::new(
        InstanceGatewayClient::new(
            input["base"].as_str().unwrap(),
            "instance_fixture".into(),
            "instance-secret-fixture".into(),
        )
        .unwrap(),
    );
    let mut results = Vec::new();
    for action in input["actions"].as_array().unwrap() {
        let result = if action["method"] == "status" {
            client
                .status()
                .await
                .map(|status| serde_json::to_value(status).unwrap())
        } else {
            client
                .onboarding_link(
                    action["returnUrl"].as_str().unwrap_or_default(),
                    action["refreshUrl"].as_str().unwrap_or_default(),
                )
                .await
                .map(|link| serde_json::to_value(link).unwrap())
        };
        results.push(match result {Ok(value)=>json!({"ok":value}),Err(error)=>json!({"error":true,"status":error.status,"retryAfterSeconds":error.retry_after_seconds,"description":error.to_string()})});
    }
    println!("{}", json!(results));
}
