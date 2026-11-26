//! Owned-memory state conformance against the actual TypeScript webhook service.
use forklaunch_billing_base::services::{
    BaseBillingService, BillingKind, BillingStore, Scope, memory::MemoryBillingStore,
};
use forklaunch_billing_stripe::{
    client::StripeBillingClient, services::StripeBillingService,
    synchronization::BillingWebhookEvent,
};
use forklaunch_core::http::provider_http::ProviderHttpClient;
use serde_json::{Value, json};
use std::{
    io::{self, Read},
    sync::Arc,
    time::Duration,
};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let input: Value = serde_json::from_str(&input)?;
    let store = Arc::new(MemoryBillingStore::new());
    let base = BaseBillingService::new(store.clone()).with_database_backup(true);
    let client = StripeBillingClient::with_transport(
        "sk_fixture".into(),
        ProviderHttpClient::new(
            input["base"].as_str().ok_or("base")?,
            Duration::from_secs(2),
            1024 * 1024,
        )?,
    )?;
    let service = StripeBillingService::new(base, client);
    let mut output = Vec::new();
    for action in input["actions"].as_array().ok_or("actions")? {
        let result = if let Some(seed) = action.get("seed") {
            let kind = kind(seed["kind"].as_str().ok_or("kind")?);
            for scope in [Scope::Durable, Scope::Cache] {
                store
                    .insert(scope, kind, seed["data"].clone(), Duration::from_secs(600))
                    .await?;
            }
            Ok(())
        } else {
            let event: BillingWebhookEvent = serde_json::from_value(action.clone())?;
            service.synchronize_event(&event, "USER").await
        };
        let mut state = serde_json::Map::new();
        for name in [
            "Plan",
            "Subscription",
            "CheckoutSession",
            "PaymentLink",
            "BillingPortal",
        ] {
            for scope in [Scope::Durable, Scope::Cache] {
                state.insert(
                    format!("{name}:{}", scope.as_str()),
                    json!(store.list(scope, kind(name), None).await?),
                );
            }
        }
        output.push(json!({"error":result.is_err(),"state":state}));
    }
    println!("{}", json!(output));
    Ok(())
}
fn kind(name: &str) -> BillingKind {
    match name {
        "Plan" => BillingKind::Plan,
        "Subscription" => BillingKind::Subscription,
        "CheckoutSession" => BillingKind::CheckoutSession,
        "PaymentLink" => BillingKind::PaymentLink,
        _ => BillingKind::BillingPortal,
    }
}
