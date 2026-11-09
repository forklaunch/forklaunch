use forklaunch_ecommerce_stripe::webhook::StripeWebhookVerifier;
use serde::Deserialize;
use std::io::{self, Read};
#[derive(Deserialize)]
struct Case {
    secret: String,
    body: String,
    header: String,
    now: u64,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut raw = String::new();
    io::stdin().read_to_string(&mut raw)?;
    let cases: Vec<Case> = serde_json::from_str(&raw)?;
    let results: Vec<_> = cases
        .into_iter()
        .map(|case| {
            StripeWebhookVerifier::new(case.secret).is_ok_and(|verifier| {
                verifier.verify_at(case.body.as_bytes(), &case.header, case.now)
            })
        })
        .collect();
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}
