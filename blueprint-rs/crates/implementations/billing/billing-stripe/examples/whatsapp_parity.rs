//! Actual TypeScript/native `WhatsApp` conformance driver.
use forklaunch_core::http::instance_gateway::InstanceGatewayClient;
use forklaunch_core::http::instance_gateway::InstanceGatewayRequestError;
use forklaunch_core::http::whatsapp_client::SendTemplateRequest;
use forklaunch_core::http::whatsapp_client::SendTextRequest;
use forklaunch_core::http::whatsapp_client::WhatsAppClient;
use serde_json::{Value, json};
use std::io::{self, Read};
#[tokio::main]
async fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let input: Value = serde_json::from_str(&input).unwrap();
    let client = WhatsAppClient::new(
        InstanceGatewayClient::new(
            input["base"].as_str().unwrap(),
            "instance_fixture".into(),
            "instance-secret-fixture".into(),
        )
        .unwrap(),
    );
    let mut results = Vec::new();
    for action in input["actions"].as_array().unwrap() {
        let result = match action["method"].as_str().unwrap() {
            "templates" => client.templates().await.map(|value| json!(value)),
            "text" => match serde_json::from_value::<SendTextRequest>(action["request"].clone()) {
                Ok(request) => client.send_text(&request).await.map(|value| json!(value)),
                Err(_) => Err(invalid()),
            },
            "template" => {
                match serde_json::from_value::<SendTemplateRequest>(action["request"].clone()) {
                    Ok(request) => client
                        .send_template(&request)
                        .await
                        .map(|value| json!(value)),
                    Err(_) => Err(invalid()),
                }
            }
            _ => panic!("unknown fixture action"),
        };
        results.push(match result{Ok(value)=>json!({"ok":value}),Err(error)=>json!({"error":true,"status":error.status,"retryAfterSeconds":error.retry_after_seconds,"description":error.to_string()})});
    }
    println!("{}", json!(results));
}
fn invalid() -> InstanceGatewayRequestError {
    let mut error = InstanceGatewayRequestError::validation("invalid_request");
    error.status = Some(400);
    error
}
