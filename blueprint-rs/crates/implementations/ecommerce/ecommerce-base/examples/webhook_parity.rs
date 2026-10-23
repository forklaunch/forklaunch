use forklaunch_ecommerce_base::webhook::InMemoryWebhookEventStore;
use forklaunch_ecommerce_interface::webhook::WebhookEventStore;
use serde_json::{Value, json};
use std::io::{self, Read};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut raw = String::new();
    io::stdin().read_to_string(&mut raw)?;
    let actions: Vec<Value> = serde_json::from_str(&raw)?;
    let store = InMemoryWebhookEventStore::default();
    let mut results = Vec::new();
    for action in actions {
        let result = if action["operation"] == "mark" {
            store
                .mark_processed(
                    action["provider"].as_str().unwrap(),
                    action["providerEventId"].as_str().unwrap(),
                )
                .await
                .map(|()| Value::Null)
        } else {
            store
                .begin_processing(serde_json::from_value(action)?)
                .await
                .map(|outcome| serde_json::to_value(outcome).unwrap())
        };
        results.push(match result {
            Ok(value) => json!({"ok":value}),
            Err(error) => json!({"error":error.code}),
        });
    }
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}
