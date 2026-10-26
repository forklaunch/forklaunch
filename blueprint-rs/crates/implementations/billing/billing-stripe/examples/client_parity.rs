//! Owned loopback transport boundary for official Stripe SDK conformance.
use forklaunch_billing_stripe::client::{StripeBillingClient, StripeResource};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use serde_json::{Value, json};
use std::{
    io::{self, Read},
    time::Duration,
};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let input: Value = serde_json::from_str(&input)?;
    let client = if input["managed"] == true {
        StripeBillingClient::managed(
            input["base"].as_str().ok_or("base")?,
            "instance_fixture".into(),
            "instance-secret-fixture".into(),
        )?
    } else {
        StripeBillingClient::with_transport(
            "sk_fixture".to_owned(),
            ProviderHttpClient::new(
                input["base"].as_str().ok_or("Missing fixture endpoint")?,
                Duration::from_millis(100),
                1024 * 1024,
            )?,
        )?
    };
    assert!(!format!("{client:?}").contains("sk_fixture"));
    let mut results = Vec::new();
    for action in input["actions"].as_array().ok_or("Missing actions")? {
        results.push(match run(&client, action).await {
            Ok(value) => json!({"ok":value}),
            Err(error) => json!({"error":error.code,"message":error.message}),
        });
    }
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}
async fn run(client: &StripeBillingClient, action: &Value) -> BlueprintResult<Value> {
    if action["resource"] == "context" {
        let context = client
            .verify_context(
                action["account"].as_str().ok_or_else(invalid)?,
                action["livemode"].as_bool().ok_or_else(invalid)?,
            )
            .await?;
        context.ensure_current(client)?;
        if let (Some(event), Some(subscription)) = (
            action["eventId"].as_str(),
            action["subscriptionId"].as_str(),
        ) {
            let (event, subscription) = client
                .retrieve_paused_effect_in_context(&context, event, subscription)
                .await?;
            return Ok(json!({"event":event,"subscription":subscription}));
        }
        if let Some(id) = action["paymentLinkId"].as_str() {
            return client.retrieve_payment_link_in_context(&context, id).await;
        }
        if let Some(id) = action["checkoutId"].as_str() {
            return client.retrieve_checkout_in_context(&context, id).await;
        }
        return Ok(json!({"account":context.account_id(),"livemode":context.livemode()}));
    }
    let resource = match action["resource"].as_str().unwrap_or("") {
        "customers" => StripeResource::Customer,
        "plans" => StripeResource::Plan,
        "products" => StripeResource::Product,
        "prices" => StripeResource::Price,
        "subscriptions" => StripeResource::Subscription,
        "checkout" => StripeResource::CheckoutSession,
        "links" => StripeResource::PaymentLink,
        "portal" => StripeResource::BillingPortal,
        _ => return Err(invalid()),
    };
    let fields = action.get("fields").cloned().unwrap_or_else(|| json!({}));
    let id = action["id"].as_str().unwrap_or("");
    match action["method"].as_str().unwrap_or("") {
        "create" => client.create(resource, &fields).await,
        "retrieve" => client.retrieve(resource, id, &fields).await,
        "update" => client.update(resource, id, &fields).await,
        "delete" => client.delete(resource, id).await,
        "list" => client.list(resource, &fields).await,
        "expire" | "resume" => {
            client
                .action(
                    resource,
                    id,
                    action["method"].as_str().ok_or_else(invalid)?,
                    &fields,
                )
                .await
        }
        _ => Err(invalid()),
    }
}
fn invalid() -> BlueprintError {
    BlueprintError::new(
        "fixture_invalid_input",
        "Invalid Stripe billing fixture input",
    )
}
