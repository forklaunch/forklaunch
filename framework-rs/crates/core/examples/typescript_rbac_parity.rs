//! Compare verified-session authorization with the actual TypeScript middleware.
use forklaunch_core::auth::{Principal, typescript::TypeScriptRbacPolicy};
use forklaunch_core::contract::AccessLevel;
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{self, Read};

#[derive(Deserialize)]
struct Case {
    principal: Principal,
    policy: TypeScriptRbacPolicy,
    access: Option<String>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let output: Vec<Value> = cases
        .into_iter()
        .map(|case| {
            let access = case.access.as_deref().map(|access| match access {
                "public" => AccessLevel::Public,
                "authenticated" => AccessLevel::Authenticated,
                "internal" => AccessLevel::Internal,
                "protected" => AccessLevel::Protected,
                _ => panic!("invalid fixture access"),
            });
            match case.policy.authorize(&case.principal, access) {
                Ok(()) => json!({"status":200,"message":null}),
                Err(error) => {
                    json!({"status":error.to_response().status_code,"message":error.message})
                }
            }
        })
        .collect();
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
