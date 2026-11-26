//! Exercise TypeScript-compatible HMAC header and verifier-dispatch semantics.
use forklaunch_core::auth::typescript::hmac::verify_hmac;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{self, Read},
};

#[derive(Deserialize)]
struct Case {
    header: Option<String>,
    prefix: Option<String>,
    keys: Option<BTreeMap<String, String>>,
    accepted: bool,
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let mut output = Vec::<Value>::new();
    for case in cases {
        let mut calls = Vec::new();
        let result = verify_hmac(case.header.as_deref(), case.prefix.as_deref(), case.keys.as_ref(), |credential| {
            calls.push(json!({"nonce":credential.nonce,"signature":credential.signature,"secretKey":credential.secret_key}));
            async { tokio::task::yield_now().await; Ok(case.accepted) }
        }).await;
        output.push(match result {
            Ok(()) => json!({"status":200,"message":null,"calls":calls,"session":null}),
            Err(error) => json!({"status":error.to_response().status_code,"message":error.message,"calls":calls,"session":null}),
        });
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
