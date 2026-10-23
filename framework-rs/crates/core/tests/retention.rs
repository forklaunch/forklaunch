use forklaunch_core::{
    common::{FrameworkError, FrameworkResult},
    persistence::{TenantContext, compliance::ComplianceMetadata, retention::*},
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
    #[compliance(level = "pii", nullable, name = "email_address")]
    email: Option<String>,
    #[compliance(level = "phi")]
    required_diagnosis: String,
    #[compliance(level = "none", nullable)]
    public_note: Option<String>,
}
#[allow(dead_code)]
#[derive(ComplianceMetadata)]
struct Audit {
    #[compliance(level = "none")]
    id: String,
}

#[derive(Clone)]
struct Row {
    tenant: &'static str,
    entity: &'static str,
    created: SystemTime,
    marked: bool,
    data: Value,
}
#[derive(Default)]
struct Store {
    rows: Mutex<Vec<Row>>,
    writes: Mutex<usize>,
    counts: Mutex<usize>,
    fail_write: Option<usize>,
    stall: bool,
    oversized: bool,
    count_value: Option<u64>,
}
fn matches(row: &Row, request: RetentionRequest<'_>) -> bool {
    row.tenant == request.tenant.id()
        && row.entity == request.policy.entity()
        && request.policy.eligible(row.created, row.marked)
}
impl RetentionStore for Store {
    fn count(
        &self,
        request: RetentionRequest<'_>,
    ) -> impl Future<Output = FrameworkResult<u64>> + Send {
        *self.counts.lock().unwrap() += 1;
        ready(Ok(self.count_value.unwrap_or_else(|| {
            self.rows
                .lock()
                .unwrap()
                .iter()
                .filter(|row| matches(row, request))
                .count() as u64
        })))
    }
    fn apply_batch(
        &self,
        request: RetentionRequest<'_>,
    ) -> impl Future<Output = FrameworkResult<usize>> + Send {
        let mut writes = self.writes.lock().unwrap();
        *writes += 1;
        if self.fail_write == Some(*writes) {
            return ready(Err(FrameworkError::new(
                "fixture_storage_failure",
                "fixture failure",
            )));
        }
        if self.stall {
            return ready(Ok(request.batch_size));
        }
        if self.oversized {
            return ready(Ok(request.batch_size + 1));
        }
        let mut rows = self.rows.lock().unwrap();
        let mut indices: Vec<_> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| matches(row, request))
            .map(|(index, row)| (index, row.created))
            .collect();
        indices.sort_by_key(|(_, created)| *created);
        indices.truncate(request.batch_size);
        let count = indices.len();
        if request.policy.action() == RetentionAction::Delete {
            indices.sort_by_key(|(index, _)| *index);
            for (index, _) in indices.into_iter().rev() {
                rows.remove(index);
            }
        } else {
            for (index, _) in indices {
                let row = &mut rows[index];
                for field in request.policy.anonymize_fields() {
                    row.data[*field] = Value::Null;
                }
                row.marked = true;
            }
        }
        ready(Ok(count))
    }
}
fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}
fn row(tenant: &'static str, entity: &'static str, created: u64, marked: bool) -> Row {
    Row {
        tenant,
        entity,
        created: at(created),
        marked,
        data: json!({"email_address":"private", "required_diagnosis":"keep-required", "public_note":"keep-public"}),
    }
}
fn tenants() -> Vec<TenantContext> {
    ["a", "b"]
        .into_iter()
        .map(|name| TenantContext::new(name).unwrap())
        .collect()
}
fn policy(action: RetentionAction) -> RetentionPolicy {
    RetentionPolicy::for_entity::<Contact>(at(10), action).unwrap()
}
// The test store returns ready futures; no background executor is needed.
fn run<F: Future>(future: F) -> F::Output {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture store must complete synchronously"),
    }
}
#[test]
fn anonymization_fences_tenants_cutoffs_and_explicit_field_classification() {
    let store = Store {
        rows: Mutex::new(vec![
            row("a", "Contact", 9, false),
            row("a", "Contact", 10, false),
            row("a", "Contact", 8, true),
            row("b", "Contact", 1, false),
            row("foreign", "Contact", 1, false),
        ]),
        ..Store::default()
    };
    let report = run(enforce(
        &store,
        &tenants(),
        &[policy(RetentionAction::Anonymize)],
        at(20),
        &EnforcementOptions {
            batch_size: 1,
            ..EnforcementOptions::default()
        },
    ))
    .unwrap();
    assert_eq!(
        (report.processed, report.anonymized, report.errors),
        (2, 2, 0)
    );
    let rows = store.rows.lock().unwrap();
    for index in [0, 3] {
        assert!(rows[index].marked);
        assert!(rows[index].data["email_address"].is_null());
        assert_eq!(rows[index].data["required_diagnosis"], "keep-required");
        assert_eq!(rows[index].data["public_note"], "keep-public");
    }
    for index in [1, 2, 4] {
        assert_eq!(rows[index].data["email_address"], "private");
    }
    drop(rows);
    assert_eq!(
        run(enforce(
            &store,
            &tenants(),
            &[policy(RetentionAction::Anonymize)],
            at(20),
            &EnforcementOptions::default()
        ))
        .unwrap()
        .processed,
        0
    );
}
#[test]
fn dry_run_counts_without_writes_and_delete_includes_anonymized_records() {
    let store = Store {
        rows: Mutex::new(vec![
            row("a", "Contact", 9, false),
            row("a", "Contact", 2, true),
            row("a", "Contact", 10, false),
        ]),
        ..Store::default()
    };
    let policies = [policy(RetentionAction::Delete)];
    let report = run(enforce(
        &store,
        &tenants(),
        &policies,
        at(20),
        &EnforcementOptions {
            dry_run: true,
            ..EnforcementOptions::default()
        },
    ))
    .unwrap();
    assert_eq!(report.deleted, 2);
    assert_eq!(*store.writes.lock().unwrap(), 0);
    assert_eq!(store.rows.lock().unwrap().len(), 3);
    let report = run(enforce(
        &store,
        &tenants(),
        &policies,
        at(20),
        &EnforcementOptions {
            batch_size: 1,
            max_batches: 2,
            ..EnforcementOptions::default()
        },
    ))
    .unwrap();
    assert_eq!((report.deleted, report.errors), (2, 0));
    assert_eq!(store.rows.lock().unwrap()[0].created, at(10));
}
#[test]
fn batch_failure_preserves_committed_counts_and_continues_other_tenants() {
    let store = Store {
        rows: Mutex::new(vec![
            row("a", "Contact", 8, false),
            row("a", "Contact", 2, false),
            row("b", "Contact", 1, false),
        ]),
        fail_write: Some(2),
        ..Store::default()
    };
    let report = run(enforce(
        &store,
        &tenants(),
        &[policy(RetentionAction::Delete)],
        at(20),
        &EnforcementOptions {
            batch_size: 1,
            ..EnforcementOptions::default()
        },
    ))
    .unwrap();
    assert_eq!((report.deleted, report.errors), (2, 1));
    assert_eq!(report.by_entity["Contact"].errors, 1);
    assert_eq!(store.rows.lock().unwrap()[0].created, at(8));
}
#[test]
fn entity_filter_and_empty_anonymization_fields_are_explicit() {
    let store = Store {
        rows: Mutex::new(vec![
            row("a", "Audit", 1, false),
            row("a", "Contact", 1, false),
        ]),
        ..Store::default()
    };
    let policies = [
        policy(RetentionAction::Delete),
        RetentionPolicy::for_entity::<Audit>(at(10), RetentionAction::Anonymize).unwrap(),
    ];
    assert!(policies[1].anonymize_fields().is_empty());
    let report = run(enforce(
        &store,
        &tenants(),
        &policies,
        at(20),
        &EnforcementOptions {
            entities: Some(vec!["Audit".into()]),
            ..EnforcementOptions::default()
        },
    ))
    .unwrap();
    assert_eq!(
        (report.anonymized, report.deleted, report.by_entity.len()),
        (1, 0, 1)
    );
    assert!(store.rows.lock().unwrap()[0].marked);
    assert!(!store.rows.lock().unwrap()[1].marked);
}
#[test]
fn invalid_scopes_fail_before_any_store_call_and_stalled_stores_are_bounded() {
    let store = Store::default();
    let policy = policy(RetentionAction::Delete);
    let options = EnforcementOptions::default();
    for (scope, policies, now, options) in [
        (vec![], vec![policy.clone()], at(20), options.clone()),
        (
            vec![tenants()[0].clone(), tenants()[0].clone()],
            vec![policy.clone()],
            at(20),
            options.clone(),
        ),
        (
            tenants(),
            vec![policy.clone(), policy.clone()],
            at(20),
            options.clone(),
        ),
        (tenants(), vec![policy.clone()], at(9), options.clone()),
        (
            tenants(),
            vec![policy.clone()],
            at(20),
            EnforcementOptions {
                batch_size: 0,
                ..options.clone()
            },
        ),
        (
            tenants(),
            vec![policy.clone()],
            at(20),
            EnforcementOptions {
                max_batches: 0,
                ..options.clone()
            },
        ),
    ] {
        assert!(run(enforce(&store, &scope, &policies, now, &options)).is_err());
    }
    assert_eq!(*store.writes.lock().unwrap(), 0);
    assert_eq!(*store.counts.lock().unwrap(), 0);
    let stalled = Store {
        stall: true,
        count_value: Some(1),
        ..Store::default()
    };
    let report = run(enforce(
        &stalled,
        &tenants()[..1],
        std::slice::from_ref(&policy),
        at(20),
        &EnforcementOptions {
            max_batches: 2,
            ..options.clone()
        },
    ))
    .unwrap();
    assert_eq!(*stalled.writes.lock().unwrap(), 2);
    assert_eq!(report.errors, 1);
    let oversized = Store {
        oversized: true,
        ..Store::default()
    };
    let report = run(enforce(
        &oversized,
        &tenants()[..1],
        &[policy],
        at(20),
        &options,
    ))
    .unwrap();
    assert_eq!((report.processed, report.errors), (0, 1));
}
#[test]
fn dry_run_counter_overflow_is_an_error() {
    let store = Store {
        count_value: Some(u64::MAX),
        ..Store::default()
    };
    assert!(
        run(enforce(
            &store,
            &tenants(),
            &[policy(RetentionAction::Delete)],
            at(20),
            &EnforcementOptions {
                dry_run: true,
                ..EnforcementOptions::default()
            }
        ))
        .is_err()
    );
}
