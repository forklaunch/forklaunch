//! Actual TypeScript `PayPal` client/service trace conformance against an owned API fixture.
use forklaunch_ecommerce_base::{BaseEcommerceService, store::InMemoryEcommerceStore};
use forklaunch_ecommerce_interface::{
    ConfirmPaymentDto, CreatePaymentDto, FailPaymentDto, PaymentService,
};
use forklaunch_ecommerce_paypal::{PaypalClient, PaypalPaymentService, VerifyWebhookSignature};
use serde_json::{Value, json};
use std::{
    io::{self, Read},
    sync::Arc,
};
fn normalize(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            fields.remove("createdAt");
            fields.remove("updatedAt");
            for value in fields.values_mut() {
                normalize(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                normalize(value);
            }
        }
        _ => {}
    }
}
async fn run_client(origin: &str, cases: &Value) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let mut client_results = Vec::new();
    for (index, case) in cases
        .as_array()
        .ok_or("Missing client cases")?
        .iter()
        .enumerate()
    {
        let client = PaypalClient::new(
            "owned-client".into(),
            "owned-secret".into(),
            &format!("{origin}/case/{index}"),
        )?;
        let result = match case["operation"].as_str().ok_or("Missing operation")? {
            "create" => client
                .create_order(
                    case["amountCents"].as_i64().ok_or("Invalid amount")?,
                    case["currency"].as_str().ok_or("Invalid currency")?,
                    case["referenceId"].as_str().ok_or("Invalid reference")?,
                )
                .await
                .and_then(|v| {
                    serde_json::to_value(v).map_err(|_| {
                        forklaunch_core::domain::BlueprintError::new("fixture", "serialize")
                    })
                }),
            "capture" => client
                .capture_order(case["id"].as_str().ok_or("Invalid id")?)
                .await
                .and_then(|v| {
                    serde_json::to_value(v).map_err(|_| {
                        forklaunch_core::domain::BlueprintError::new("fixture", "serialize")
                    })
                }),
            "verify" => {
                let params: VerifyWebhookSignature =
                    serde_json::from_value(case["params"].clone())?;
                client
                    .verify_webhook_signature(&params)
                    .await
                    .map(Value::Bool)
            }
            _ => return Err("Unknown operation".into()),
        };
        client_results.push(match result {
            Ok(value) => json!({"ok":value}),
            Err(_) => json!({"error":true}),
        });
    }
    Ok(client_results)
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let origin = std::env::args().nth(1).ok_or("Missing fixture endpoint")?;
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let fixture: Value = serde_json::from_str(&input)?;
    let client_results = run_client(&origin, &fixture["client"]).await?;
    let service = PaypalPaymentService::new(
        BaseEcommerceService::new(Arc::new(InMemoryEcommerceStore::default())),
        PaypalClient::new(
            "owned-client".into(),
            "owned-secret".into(),
            &format!("{origin}/service"),
        )?,
    );
    let mut service_results = Vec::new();
    for case in fixture["service"]
        .as_array()
        .ok_or("Missing service cases")?
    {
        let result = match case["operation"].as_str().ok_or("Missing operation")? {
            "create" => {
                let request: CreatePaymentDto = serde_json::from_value(case["input"].clone())?;
                service.create_payment(request).await
            }
            "confirm" => {
                service
                    .confirm_payment(ConfirmPaymentDto {
                        provider_ref: case["input"]["providerRef"]
                            .as_str()
                            .ok_or("Invalid ref")?
                            .into(),
                    })
                    .await
            }
            "fail" => {
                service
                    .fail_payment(FailPaymentDto {
                        provider_ref: case["input"]["providerRef"]
                            .as_str()
                            .ok_or("Invalid ref")?
                            .into(),
                    })
                    .await
            }
            "get" => {
                service
                    .get_payment(case["input"]["id"].as_str().ok_or("Invalid id")?)
                    .await
            }
            _ => return Err("Unknown operation".into()),
        };
        let mut output = match result {
            Ok(value) => json!({"ok":value}),
            Err(_) => json!({"error":true}),
        };
        normalize(&mut output);
        service_results.push(output);
    }
    println!(
        "{}",
        json!({"client":client_results,"service":service_results})
    );
    Ok(())
}
