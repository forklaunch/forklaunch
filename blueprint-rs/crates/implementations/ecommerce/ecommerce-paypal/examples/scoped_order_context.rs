//! Owned loopback wire/schema oracle; never points at `PayPal` by default.
use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_ecommerce_paypal::{PaypalClient, context::PaypalEnvironment};
use serde_json::{Value, json};
use std::{
    io::{self, Read},
    time::Duration,
};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let origin = std::env::args().nth(1).ok_or("owned origin required")?;
    if !origin.starts_with("http://127.0.0.1:") {
        return Err("owned loopback origin required".into());
    }
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Value> = serde_json::from_str(&input)?;
    let mut results = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let transport = ProviderHttpClient::new(
            &format!("{origin}/case/{index}"),
            Duration::from_secs(2),
            4096,
        )?;
        let client = PaypalClient::with_trusted_environment_transport(
            "owned-client".into(),
            "owned-secret".into(),
            PaypalEnvironment::Sandbox,
            transport,
        )?;
        let merchant = case["merchant"].as_str().ok_or("merchant required")?;
        match client
            .verify_order_context(PaypalEnvironment::Sandbox, merchant, "ORDER_owned")
            .await
        {
            Ok(context) => results.push(json!({"ok":context.observation()})),
            Err(error) => results.push(json!({"error":error.code})),
        }
    }
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}
