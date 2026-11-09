use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_ecommerce_base::shipping::FlatRateShippingService;
use forklaunch_ecommerce_interface::pricing::{
    ShippingCalculation, ShippingService, TaxCalculation, TaxService,
};
use forklaunch_ecommerce_stripe::{StripeClient, tax::StripeTaxService};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::{self, Read},
    time::Duration,
};
#[derive(Deserialize)]
struct Input {
    base: String,
    cases: Vec<Value>,
}
#[tokio::main]
async fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let input: Input = serde_json::from_str(&input).unwrap();
    let mut results = Vec::new();
    for case in input.cases {
        let value = if case["operation"] == "shipping" {
            let request: ShippingCalculation =
                serde_json::from_value(case["request"].clone()).unwrap();
            FlatRateShippingService
                .calculate(request)
                .await
                .map(|value| serde_json::to_value(value).unwrap())
        } else {
            let request: TaxCalculation = serde_json::from_value(case["request"].clone()).unwrap();
            let http = ProviderHttpClient::new(
                &format!("{}/{}", input.base, case["scenario"].as_str().unwrap()),
                Duration::from_secs(2),
                4096,
            )
            .unwrap();
            StripeTaxService::new(
                StripeClient::with_transport("sk_fixture".to_owned(), http).unwrap(),
            )
            .calculate(request)
            .await
            .map(|value| serde_json::to_value(value).unwrap())
        };
        results.push(match value {
            Ok(value) => json!({"ok":value}),
            Err(error) => json!({"error":error.code}),
        });
    }
    println!("{}", serde_json::to_string(&results).unwrap());
}
