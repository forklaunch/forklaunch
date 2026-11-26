//! Owned model gateway wire fixture; no provider credentials or external calls.
use forklaunch_core::http::instance_gateway::InstanceGatewayClient;
use forklaunch_core::http::model_gateway::GatewayChatChunk;
use forklaunch_core::http::model_gateway::GatewayChatCompletion;
use forklaunch_core::http::model_gateway::GatewayChatRequest;
use forklaunch_core::http::model_gateway::GatewayModels;
use forklaunch_core::http::model_gateway::ModelGatewayClient;
use forklaunch_core::http::model_gateway::ModelStreamLimits;
use serde_json::{Value, json};
use std::io::{self, Read};

#[tokio::main]
async fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let input: Value = serde_json::from_str(&input).unwrap();
    let transport = InstanceGatewayClient::new(
        input["base"].as_str().unwrap(),
        "model-fixture".into(),
        "model-only-fixture-secret".into(),
    )
    .unwrap();
    let client = ModelGatewayClient::new(transport);
    let mut results = Vec::new();
    for action in input["actions"].as_array().unwrap() {
        if action["kind"] == "dto" {
            results.push(dto(action));
            continue;
        }
        if action["kind"] == "stream" {
            results.push(stream(&client, action).await);
            continue;
        }
        let result = match action["kind"].as_str().unwrap() {
            "models" => client.models().await,
            "chat" => client.chat_completion(&action["body"]).await,
            _ => panic!("unknown fixture action"),
        };
        results.push(match result {
            Ok(value) => json!({"ok":value}),
            Err(error) => json!({"error":true,"status":error.status,
                "retryAfterSeconds":error.retry_after_seconds,"code":error.code,
                "description":error.to_string()}),
        });
    }
    println!("{}", json!(results));
}

async fn stream(client: &ModelGatewayClient, action: &Value) -> Value {
    let mut items = Vec::new();
    let limits = if let Some(limits) = action.get("limits") {
        ModelStreamLimits::new(
            usize::try_from(limits["total"].as_u64().unwrap()).unwrap(),
            usize::try_from(limits["frame"].as_u64().unwrap()).unwrap(),
            usize::try_from(limits["frames"].as_u64().unwrap()).unwrap(),
            std::time::Duration::from_millis(limits["millis"].as_u64().unwrap()),
        )
    } else {
        Ok(ModelStreamLimits::default())
    };
    let result = async {
        let limits = limits
            .map_err(forklaunch_core::http::instance_gateway::InstanceGatewayRequestError::from)?;
        let mut stream = client
            .chat_completion_stream_with_limits(&action["body"], limits)
            .await?;
        while let Some(item) = stream.next().await? {
            items.push(item);
            if action["cancel_after"]
                .as_u64()
                .is_some_and(|count| u64::try_from(items.len()).unwrap() >= count)
            {
                if action["drop_only"] != true {
                    stream.cancel();
                    assert!(stream.next().await?.is_none());
                }
                break;
            }
        }
        Ok::<(), forklaunch_core::http::instance_gateway::InstanceGatewayRequestError>(())
    }
    .await;
    match result {
        Ok(()) => json!({"ok":items}),
        Err(error) => {
            json!({"error":true,"status":error.status,"retryAfterSeconds":error.retry_after_seconds,"code":error.code,"description":error.to_string(),"items":items})
        }
    }
}

fn dto(action: &Value) -> Value {
    fn roundtrip<T: serde::de::DeserializeOwned + serde::Serialize>(value: Value) -> Value {
        serde_json::from_value::<T>(value)
            .and_then(serde_json::to_value)
            .unwrap()
    }
    match action["shape"].as_str().unwrap() {
        "request" => roundtrip::<GatewayChatRequest>(action["body"].clone()),
        "completion" => roundtrip::<GatewayChatCompletion>(action["body"].clone()),
        "chunk" => roundtrip::<GatewayChatChunk>(action["body"].clone()),
        "models" => roundtrip::<GatewayModels>(action["body"].clone()),
        _ => panic!("unknown DTO fixture shape"),
    }
}
