//! Owned actual-source wire fixture; no provider credentials or external calls.
use forklaunch_core::http::instance_gateway::InstanceGatewayClient;
use forklaunch_core::http::instance_gateway::InstanceGatewayRequestError;
use forklaunch_core::http::preview_model_gateway::PreviewModelGatewayClient;
use reqwest::Method;
use serde_json::{Value, json};
use std::io::{self, Read};
#[tokio::main]
async fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let input: Value = serde_json::from_str(&input).unwrap();
    let transport = InstanceGatewayClient::new(
        input["base"].as_str().unwrap(),
        "preview-fixture".into(),
        "preview-only-fixture-secret".into(),
    )
    .unwrap();
    let now = input["now"].as_i64().unwrap();
    let client = PreviewModelGatewayClient::new(transport.clone()).with_clock(move || now);
    let mut results = Vec::new();
    for action in input["actions"].as_array().unwrap() {
        let result = match action["kind"].as_str().unwrap() {
            "models" => client.models().await.map(|value| json!(value)),
            "chat" => client.chat_completion(&action["body"]).await,
            "default" => transport
                .request_json(Method::GET, &["models"], None)
                .await
                .map(|response| response.body)
                .map_err(InstanceGatewayRequestError::from),
            "invalid-limit" => transport
                .request_json_with_response_limit(
                    Method::GET,
                    &["models"],
                    None,
                    usize::try_from(action["limit"].as_u64().unwrap()).unwrap(),
                )
                .await
                .map(|response| response.body)
                .map_err(InstanceGatewayRequestError::from),
            _ => panic!("unknown fixture action"),
        };
        results.push(match result {
            Ok(value) => {
                if let Some(number) = value.get("number").and_then(Value::as_number) {
                    json!({"ok":value,"numberBits":format!("{:016x}",number.as_f64().unwrap().to_bits()),"numberFloat":number.is_f64()})
                } else { json!({"ok":value}) }
            },
            Err(error)=>json!({"error":true,"status":error.status,"retryAfterSeconds":error.retry_after_seconds,"code":error.code,"description":error.to_string()}),
        });
    }
    println!("{}", json!(results));
}
