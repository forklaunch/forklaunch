//! Compare opt-in Basic verification against the actual TypeScript middleware.
use forklaunch_core::auth::typescript::basic::{verify_basic, verify_basic_resource};
use forklaunch_core::auth::typescript::jwt::verify_jwt_resource;
use forklaunch_core::http::{ErrorKind, ErrorResponse};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{self, Read};

#[derive(Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Method {
    #[default]
    Basic,
    Jwt,
}

#[derive(Deserialize)]
struct Case {
    header: Option<String>,
    prefix: Option<String>,
    accepted: bool,
    #[serde(default)]
    resource: bool,
    #[serde(default)]
    claims: Value,
    #[serde(default)]
    method: Method,
    #[serde(default)]
    fail: bool,
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let mut output: Vec<Value> = Vec::new();
    for case in cases {
        if case.resource {
            let mut calls = Vec::new();
            let decode = |token: &str| {
                calls.push(json!(token));
                async move {
                    tokio::task::yield_now().await;
                    if case.fail {
                        Err(ErrorResponse::new(
                            ErrorKind::Internal,
                            "private decoder failure",
                        ))
                    } else {
                        Ok(case.claims)
                    }
                }
            };
            let result = if matches!(case.method, Method::Jwt) {
                verify_jwt_resource(case.header.as_deref(), case.prefix.as_deref(), decode).await
            } else {
                verify_basic_resource(case.header.as_deref(), case.prefix.as_deref(), decode).await
            };
            output.push(match result {
                Ok(claims) => json!({"status":200,"message":null,"claims":claims,"calls":calls}),
                Err(error) => json!({"status":error.to_response().status_code,"message":error.message,"claims":null,"calls":calls}),
            });
            continue;
        }
        let mut calls = Vec::new();
        let result = verify_basic(
            case.header.as_deref(),
            case.prefix.as_deref(),
            |user, password| {
                calls.push(json!([user, password]));
                case.accepted
            },
        );
        output.push(match result {
            Ok(principal) => json!({"status":200,"message":null,"subject":principal.subject,"calls":calls}),
            Err(error) => json!({"status":error.to_response().status_code,"message":error.message,"subject":null,"calls":calls}),
        });
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
