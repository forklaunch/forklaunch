//! In-memory conformance adapter; production stores must supply transactions.
use forklaunch_core::{
    common::FrameworkResult,
    persistence::{
        TenantContext,
        compliance::ComplianceMetadata,
        retention::{
            EnforcementOptions, RetentionAction, RetentionPolicy, RetentionRequest, RetentionStore,
            enforce,
        },
    },
};
use serde_json::{Value, json};
use std::{
    future::{Future, ready},
    pin::pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
    time::{Duration, SystemTime},
};

#[allow(dead_code)]
#[derive(ComplianceMetadata)]
struct Contact {
    #[compliance(level = "pii", nullable, name = "email")]
    email: Option<String>,
    #[compliance(level = "phi")]
    diagnosis: String,
    #[compliance(level = "none", nullable)]
    note: Option<String>,
}
struct Store(Mutex<Vec<Value>>);
fn eligible(row: &Value, request: RetentionRequest<'_>) -> bool {
    row["tenant"].as_str() == Some(request.tenant.id())
        && request.policy.eligible(
            SystemTime::UNIX_EPOCH + Duration::from_secs(row["created"].as_u64().unwrap()),
            row["marked"].as_bool().unwrap(),
        )
}
impl RetentionStore for Store {
    fn count(
        &self,
        request: RetentionRequest<'_>,
    ) -> impl Future<Output = FrameworkResult<u64>> + Send {
        ready(Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|row| eligible(row, request))
            .count() as u64))
    }
    fn apply_batch(
        &self,
        request: RetentionRequest<'_>,
    ) -> impl Future<Output = FrameworkResult<usize>> + Send {
        let mut rows = self.0.lock().unwrap();
        let mut selected: Vec<_> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| eligible(row, request))
            .map(|(index, row)| (index, row["created"].as_u64().unwrap()))
            .collect();
        selected.sort_by_key(|(_, created)| *created);
        selected.truncate(request.batch_size);
        let count = selected.len();
        if request.policy.action() == RetentionAction::Delete {
            selected.sort_by_key(|(index, _)| *index);
            for (index, _) in selected.into_iter().rev() {
                rows.remove(index);
            }
        } else {
            for (index, _) in selected {
                for field in request.policy.anonymize_fields() {
                    rows[index][*field] = Value::Null;
                }
                rows[index]["marked"] = json!(true);
            }
        }
        ready(Ok(count))
    }
}
fn main() {
    let input: Vec<Value> = serde_json::from_reader(std::io::stdin()).unwrap();
    let mut results = Vec::new();
    for case in input {
        let store = Store(Mutex::new(case["rows"].as_array().unwrap().clone()));
        let tenants: Vec<_> = case["tenants"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| TenantContext::new(value.as_str().unwrap()).unwrap())
            .collect();
        let action = if case["action"] == "delete" {
            RetentionAction::Delete
        } else {
            RetentionAction::Anonymize
        };
        let policies = [RetentionPolicy::for_entity::<Contact>(
            SystemTime::UNIX_EPOCH + Duration::from_secs(10),
            action,
        )
        .unwrap()];
        let options = EnforcementOptions {
            dry_run: case["dryRun"].as_bool().unwrap(),
            batch_size: case["batchSize"].as_u64().unwrap().try_into().unwrap(),
            ..EnforcementOptions::default()
        };
        let future = enforce(
            &store,
            &tenants,
            &policies,
            SystemTime::UNIX_EPOCH + Duration::from_secs(86410),
            &options,
        );
        let Poll::Ready(report) = pin!(future).poll(&mut Context::from_waker(Waker::noop())) else {
            panic!("fixture adapter must return ready futures")
        };
        let report = report.unwrap();
        let counts = &report.by_entity["Contact"];
        results.push(json!({"processed":report.processed,"deleted":report.deleted,"anonymized":report.anonymized,"errors":report.errors,"byEntity":{"Contact":{"deleted":counts.deleted,"anonymized":counts.anonymized,"errors":counts.errors}},"rows":*store.0.lock().unwrap()}));
    }
    println!("{}", serde_json::to_string(&results).unwrap());
}
