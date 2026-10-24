//! Bound borrowed input before recursive serialization, cloning or canonicalization.
use super::{StripeBindingScope, invalid};
use forklaunch_core::domain::BlueprintResult;
use serde::Serialize;
use serde_json::Value;
use std::io::{self, Write};
use uuid::Uuid;

const LIMIT: usize = 65_536;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<'a> {
    tenant: &'a str,
    operation: String,
    deployment: &'a str,
    account: &'a str,
    livemode: bool,
    api_version: &'a str,
    payload: &'a Value,
}

// This traversal never clones input, and stops before descending beyond its limit.
fn shape(value: &Value, depth: usize, nodes: &mut usize) -> BlueprintResult<()> {
    if depth > 32 || *nodes >= 8192 {
        return Err(invalid());
    }
    *nodes += 1;
    match value {
        Value::Array(values) => {
            for value in values {
                shape(value, depth + 1, nodes)?;
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                if key.len() > LIMIT {
                    return Err(invalid());
                }
                shape(value, depth + 1, nodes)?;
            }
        }
        Value::String(value) if value.len() > LIMIT => return Err(invalid()),
        _ => {}
    }
    Ok(())
}

struct Counter(usize);
impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > LIMIT - self.0 {
            return Err(io::Error::other("intent exceeds admission bound"));
        }
        self.0 += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn validate(
    tenant: &str,
    scope: &StripeBindingScope,
    operation: Uuid,
    api_version: &str,
    payload: &Value,
) -> BlueprintResult<()> {
    // Envelope root plus its six non-payload values are counted too.
    shape(payload, 1, &mut 7)?;
    let envelope = Envelope {
        tenant,
        operation: operation.to_string(),
        deployment: &scope.deployment_label,
        account: &scope.account_id,
        livemode: scope.livemode,
        api_version,
        payload,
    };
    serde_json::to_writer(Counter(0), &envelope).map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn accepts(payload: &Value) -> bool {
        validate("tenant", &scope(), Uuid::nil(), "version", payload).is_ok()
    }
    fn scope() -> StripeBindingScope {
        StripeBindingScope::administrative("deployment".into(), "acct_owned".into(), false).unwrap()
    }
    #[test]
    fn bounds_shape_before_serialization() {
        let mut deep = Value::Null;
        for _ in 0..31 {
            deep = Value::Array(vec![deep]);
        }
        assert!(accepts(&deep));
        deep = Value::Array(vec![deep]);
        assert!(!accepts(&deep));
        assert!(accepts(&json!(vec![0; 8184])));
        assert!(!accepts(&json!(vec![0; 8185])));
        assert!(!accepts(&json!(vec![0; 8192])));
        assert!(!accepts(&json!("x".repeat(LIMIT + 1))));
        assert!(!accepts(&json!({"x".repeat(LIMIT + 1): 0})));
        assert!(!accepts(&json!("\u{0000}".repeat(12_000))));
        assert!(!accepts(&json!({"\u{0000}".repeat(12_000): 0})));
        assert!(accepts(&json!("x".repeat(60_000))));
        assert!(accepts(&json!("🦀".repeat(15_000))));
    }
    #[test]
    fn exact_envelope_bytes_and_canonical_hash_are_preserved() {
        let scope = scope();
        let operation = Uuid::nil();
        let mut payload = json!({"z":[1,1.25,true,null],"a":"é\n\""});
        let envelope = |payload: &Value| json!({"tenant":"tenant","operation":operation.to_string(),"deployment":scope.deployment_label,"account":scope.account_id,"livemode":scope.livemode,"apiVersion":"version","payload":payload});
        let before = super::super::canonical(&envelope(&payload)).to_string();
        assert!(accepts(&payload));
        assert_eq!(
            before,
            super::super::canonical(&envelope(&payload)).to_string()
        );
        assert_eq!(
            super::super::intent_hash("public", &before),
            "d10269841bfbca259d032f263e9dbe26ad5efef9b1a7371f00ad22b58505e415"
        );
        payload = json!("");
        let overhead = envelope(&payload).to_string().len();
        payload = json!("x".repeat(LIMIT - overhead));
        assert!(accepts(&payload));
        payload = json!("x".repeat(LIMIT - overhead + 1));
        assert!(!accepts(&payload));
    }
}
