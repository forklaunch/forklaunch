//! Consumes the actual TypeScript provider fixture's scenarios against its owned HTTP fake.
use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_ecommerce_base::{BaseEcommerceService, store::InMemoryEcommerceStore};
use forklaunch_ecommerce_interface::{
    ConfirmPaymentDto, CreatePaymentDto, FailPaymentDto, PaymentService,
};
use forklaunch_ecommerce_stripe::{StripeClient, StripeConnect, StripePaymentService};
use serde_json::{Value, json};
use std::{
    io::{self, Read},
    sync::Arc,
    time::Duration,
};
fn normalize(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            fields.remove("createdAt");
            fields.remove("updatedAt");
            for v in fields.values_mut() {
                normalize(v);
            }
        }
        Value::Array(values) => {
            for v in values {
                normalize(v);
            }
        }
        _ => {}
    }
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let origin = std::env::args().nth(1).ok_or("Missing fixture endpoint")?;
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Value> = serde_json::from_str(&input)?;
    let mut results = Vec::new();
    for case in cases {
        let build=||->Result<StripePaymentService<InMemoryEcommerceStore>,Box<dyn std::error::Error>>{
        let base=BaseEcommerceService::new(Arc::new(InMemoryEcommerceStore::default()));
        let client=StripeClient::with_transport("owned-test-stripe-secret".into(),ProviderHttpClient::new(&origin,Duration::from_secs(5),1024*1024)?)?;
        let mut service=StripePaymentService::new(base,client);
        if let Some(connect)=case.get("connect"){let bps: u16=serde_json::from_value(connect["platformFeeBps"].clone())?;service=service.with_connect(StripeConnect::new(connect["connectedAccountId"].as_str().ok_or("Missing account")?.into(),bps)?);}
        Ok(service)
    };
        let Ok(service) = build() else {
            results.push(json!({"error":true}));
            continue;
        };
        let request: CreatePaymentDto = serde_json::from_value(case["request"].clone())?;
        let id = request.id.clone().ok_or("Missing id")?;
        if case["providerless"] == true {
            service.base().create_payment(request).await?;
            let mut output = json!({"resumed":service.resume_payment(&id).await?});
            normalize(&mut output);
            results.push(output);
            continue;
        }
        let created = service.create_payment(request).await?;
        let resumed = service.resume_payment(&id).await?;
        let fetched = service.get_payment(&id).await?;
        let provider_ref = created
            .provider_ref
            .clone()
            .ok_or("Missing provider reference")?;
        let confirmed = service
            .confirm_payment(ConfirmPaymentDto {
                provider_ref: provider_ref.clone(),
            })
            .await?;
        let failed = service
            .fail_payment(FailPaymentDto { provider_ref })
            .await?;
        let mut output = json!({"created":created,"resumed":resumed,"fetched":fetched,"confirmed":confirmed,"lateFailure":failed});
        normalize(&mut output);
        results.push(output);
    }
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}
