//! Compare ordered callbacks against the actual TypeScript auth middleware.
use forklaunch_core::auth::typescript::{TypeScriptRbacPolicy, session::TypeScriptSessionResolver};
use forklaunch_core::contract::AccessLevel;
use forklaunch_core::http::{ErrorKind, ErrorResponse};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::{self, Read},
    sync::Mutex,
};

#[derive(Deserialize)]
struct Case {
    policy: TypeScriptRbacPolicy,
    access: Option<String>,
    claims: Value,
    callbacks: Vec<String>,
    grants: Value,
    validation: Option<bool>,
    fail: Option<String>,
}
struct Resolver<'a> {
    case: &'a Case,
    calls: Mutex<Vec<String>>,
}
impl Resolver<'_> {
    async fn resolve(&self, name: &str, claims: &Value) -> Result<Option<Value>, ErrorResponse> {
        assert_eq!(
            claims, &self.case.claims,
            "validation must preserve original claims"
        );
        if !self.case.callbacks.iter().any(|callback| callback == name) {
            return Ok(None);
        }
        self.calls.lock().unwrap().push(name.to_owned());
        tokio::task::yield_now().await;
        if self.case.fail.as_deref() == Some(name) {
            return Err(ErrorResponse::new(
                ErrorKind::Internal,
                "public callback error",
            ));
        }
        Ok(Some(self.case.grants[name].clone()))
    }
}
macro_rules! grant {
    ($name:ident, $ty:ty) => {
        async fn $name(&self, claims: &Value) -> Result<Option<$ty>, ErrorResponse> {
            self.resolve(stringify!($name), claims)
                .await
                .map(|value| value.map(|value| serde_json::from_value(value).unwrap()))
        }
    };
}
impl TypeScriptSessionResolver for Resolver<'_> {
    fn validate_session(&self, claims: &Value) -> Result<(), ErrorResponse> {
        assert_eq!(claims, &self.case.claims);
        if let Some(valid) = self.case.validation {
            self.calls.lock().unwrap().push("validate".into());
            if !valid {
                return Err(ErrorResponse::new(
                    ErrorKind::Validation,
                    "Invalid session: Session validation failed",
                ));
            }
        }
        Ok(())
    }
    grant!(scopes, Vec<String>);
    grant!(permissions, Vec<String>);
    grant!(roles, Vec<String>);
    grant!(subscription, bool);
    grant!(features, Vec<String>);
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Case> = serde_json::from_str(&input)?;
    let mut output = Vec::new();
    for case in cases {
        let access = case.access.as_deref().map(|access| match access {
            "public" => AccessLevel::Public,
            "authenticated" => AccessLevel::Authenticated,
            "internal" => AccessLevel::Internal,
            "protected" => AccessLevel::Protected,
            _ => panic!("invalid access"),
        });
        let resolver = Resolver {
            case: &case,
            calls: Mutex::new(Vec::new()),
        };
        let (status, message) = match case
            .policy
            .authorize_session(&case.claims, access, &resolver)
            .await
        {
            Ok(()) => (200, None),
            Err(error) => (error.to_response().status_code, Some(error.message)),
        };
        output.push(
            json!({"status":status,"message":message,"calls":*resolver.calls.lock().unwrap()}),
        );
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
