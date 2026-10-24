// Immediate mock adapters preserve the same async trait signatures as storage adapters.
#![allow(clippy::unused_async_trait_impl)]
use forklaunch_core::{
    common::{FrameworkError, FrameworkResult},
    persistence::{TenantContext, compliance::ComplianceMetadata, subject::*},
};
use std::{
    collections::BTreeSet,
    future::Future,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Waker},
};
#[derive(ComplianceMetadata)]
#[compliance(entity = "Record")]
#[allow(dead_code)]
struct Record {
    #[compliance(level = "none")]
    subject: String,
    #[compliance(level = "pii")]
    body: String,
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("unexpected async mock"),
    }
}
fn error() -> FrameworkError {
    FrameworkError::new("failure", "sensitive adapter details must not escape")
}
#[derive(Default)]
struct Store {
    checkpoint: Mutex<usize>,
    fail_ack: AtomicBool,
    fail_erase: AtomicBool,
}
impl SubjectExportStore for Store {
    type Record = String;
    async fn next_batch(
        &self,
        r: &SubjectRequest<'_>,
    ) -> FrameworkResult<Option<ExportBatch<String>>> {
        assert_eq!(r.tenant.id(), "tenant-a");
        assert_eq!(r.subject, "subject-a");
        let index = *self.checkpoint.lock().unwrap();
        Ok((index < 2).then(|| ExportBatch {
            batch_id: index.to_string(),
            records: vec![format!("private-{index}")],
        }))
    }
    async fn acknowledge(&self, _: &SubjectRequest<'_>, id: &str) -> FrameworkResult<bool> {
        if self.fail_ack.swap(false, Ordering::SeqCst) {
            return Err(error());
        }
        let mut index = self.checkpoint.lock().unwrap();
        if id == index.to_string() {
            *index += 1;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
impl SubjectEraseStore for Store {
    async fn erase_batch(&self, _: &SubjectRequest<'_>) -> FrameworkResult<EraseBatch> {
        if self.fail_erase.swap(false, Ordering::SeqCst) {
            return Err(error());
        }
        let mut index = self.checkpoint.lock().unwrap();
        if *index == 2 {
            return Ok(EraseBatch {
                deleted: 0,
                complete: true,
            });
        }
        *index += 1;
        Ok(EraseBatch {
            deleted: 1,
            complete: *index == 2,
        })
    }
}
#[derive(Default)]
struct Sink {
    received: Mutex<BTreeSet<String>>,
    fail: AtomicBool,
}
impl SubjectExportSink<String> for Sink {
    async fn deliver(
        &self,
        r: &SubjectRequest<'_>,
        b: &ExportBatch<String>,
    ) -> FrameworkResult<()> {
        if self.fail.swap(false, Ordering::SeqCst) {
            return Err(error());
        }
        self.received.lock().unwrap().insert(format!(
            "{}:{}:{}:{}",
            r.tenant.id(),
            r.subject,
            r.operation_id,
            b.batch_id
        ));
        Ok(())
    }
}
#[test]
fn export_checkpoint_failure_replays_idempotently_without_losing_records() {
    let tenant = TenantContext::new("tenant-a").unwrap();
    let entity = SubjectEntity::for_entity::<Record>("subject").unwrap();
    let request = SubjectRequest {
        tenant: &tenant,
        subject: "subject-a",
        entity: &entity,
        operation_id: "export-1",
        batch_size: 1,
    };
    let store = Store::default();
    let sink = Sink::default();
    store.fail_ack.store(true, Ordering::SeqCst);
    let result = ready(export_subject(&store, &sink, &request, 4)).unwrap();
    assert_eq!(result.incomplete, Some(IncompleteReason::Storage));
    assert_eq!(result.records, 0);
    assert!(!format!("{result:?}").contains("sensitive"));
    let resumed = ready(export_subject(&store, &sink, &request, 4)).unwrap();
    assert!(resumed.complete);
    assert_eq!(resumed.records, 2);
    assert_eq!(sink.received.lock().unwrap().len(), 2);
    assert_eq!(
        ready(export_subject(&store, &sink, &request, 4))
            .unwrap()
            .records,
        0
    );
}
#[test]
fn failed_destination_does_not_advance_and_limits_report_incomplete() {
    let tenant = TenantContext::new("tenant-a").unwrap();
    let entity = SubjectEntity::for_entity::<Record>("subject").unwrap();
    let request = SubjectRequest {
        tenant: &tenant,
        subject: "subject-a",
        entity: &entity,
        operation_id: "export-2",
        batch_size: 1,
    };
    let store = Store::default();
    let sink = Sink::default();
    sink.fail.store(true, Ordering::SeqCst);
    assert_eq!(
        ready(export_subject(&store, &sink, &request, 1))
            .unwrap()
            .incomplete,
        Some(IncompleteReason::Destination)
    );
    assert_eq!(*store.checkpoint.lock().unwrap(), 0);
    assert_eq!(
        ready(export_subject(&store, &sink, &request, 1))
            .unwrap()
            .incomplete,
        Some(IncompleteReason::BatchLimit)
    );
    assert!(
        ready(export_subject(&store, &sink, &request, 2))
            .unwrap()
            .complete
    );
}
#[test]
fn erase_preserves_progress_and_retries_without_double_counting() {
    let tenant = TenantContext::new("tenant-a").unwrap();
    let entity = SubjectEntity::for_entity::<Record>("subject").unwrap();
    let request = SubjectRequest {
        tenant: &tenant,
        subject: "subject-a",
        entity: &entity,
        operation_id: "erase-1",
        batch_size: 1,
    };
    let store = Store::default();
    let first = ready(erase_subject(&store, &request, 1)).unwrap();
    assert_eq!(first.records, 1);
    assert!(!first.complete);
    store.fail_erase.store(true, Ordering::SeqCst);
    assert_eq!(
        ready(erase_subject(&store, &request, 2))
            .unwrap()
            .incomplete,
        Some(IncompleteReason::Storage)
    );
    let resumed = ready(erase_subject(&store, &request, 2)).unwrap();
    assert!(resumed.complete);
    assert_eq!(resumed.records, 1);
    assert_eq!(
        ready(erase_subject(&store, &request, 2)).unwrap().records,
        0
    );
    assert!(SubjectEntity::for_entity::<Record>("guessed_owner").is_err());
    assert!(ready(erase_subject(&store, &request, 0)).is_err());
}
