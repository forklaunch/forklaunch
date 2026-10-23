//! Actual TypeScript/native managed voice conformance driver.
use forklaunch_core::http::instance_gateway::InstanceGatewayClient;
use forklaunch_core::http::instance_gateway::InstanceGatewayRequestError;
use forklaunch_core::http::voice_client::StartOutboundCallRequest;
use forklaunch_core::http::voice_client::VoiceClient;
use forklaunch_core::http::voice_client::voice_request_problem;
use serde_json::{Value, json};
use std::io::{self, Read};
#[tokio::main]
async fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let input: Value = serde_json::from_str(&input).unwrap();
    let client = VoiceClient::new(
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
            "call" => client
                .call(action["id"].as_str().unwrap())
                .await
                .map(|value| json!(value)),
            "end" => client
                .end_call(action["id"].as_str().unwrap())
                .await
                .map(|()| Value::Null),
            "start" => {
                match serde_json::from_value::<StartOutboundCallRequest>(action["request"].clone())
                {
                    Ok(request) => {
                        if action["invalid"] == true {
                            assert!(voice_request_problem(&request).is_some());
                        }
                        client
                            .start_outbound_call(&request)
                            .await
                            .map(|value| json!(value))
                    }
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
