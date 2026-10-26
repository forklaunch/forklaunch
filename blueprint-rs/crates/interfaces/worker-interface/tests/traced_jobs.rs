#![cfg(all(feature = "encrypted", feature = "telemetry"))]
use forklaunch_core::encryption::{EnvelopeKey, EnvelopeProfile};
use forklaunch_worker_interface::encrypted::{
    JobAccess, JobAccessAudit, JobContext, JobPayload, ProcessorError, TraceCarrier, TracedJob,
    process_traced_audited, seal_job,
};
use opentelemetry::{
    Context,
    trace::{TraceContextExt, TraceId},
};
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider};
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use uuid::Uuid;
use zeroize::Zeroize;
const PARENT: &str = "00-11111111111111111111111111111111-2222222222222222-01";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
struct Payload {
    context: JobContext,
    secret: String,
}
impl Zeroize for Payload {
    fn zeroize(&mut self) {
        self.secret.zeroize();
    }
}
impl JobPayload for Payload {
    fn context(&self) -> JobContext {
        self.context
    }
}
struct Audit {
    calls: AtomicUsize,
    fail: bool,
}
impl JobAccessAudit for Audit {
    fn record<'a>(
        &'a self,
        _: JobAccess<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ProcessorError>> + Send + 'a>> {
        Box::pin(async move {
            assert_ne!(
                Context::current().span().span_context().trace_id(),
                TraceId::from_hex(&PARENT[3..35]).unwrap()
            );
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                Err(ProcessorError::Failed)
            } else {
                Ok(())
            }
        })
    }
}
struct NeverAck;
impl JobAccessAudit for NeverAck {
    fn record<'a>(
        &'a self,
        _: JobAccess<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ProcessorError>> + Send + 'a>> {
        Box::pin(std::future::pending())
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // One provider isolates successful and rejected propagation paths.
async fn remote_parent_is_imported_only_after_ciphertext_context_and_audit_ack() {
    let spans = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(spans.clone())
        .build();
    opentelemetry::global::set_tracer_provider(provider.clone());
    let key = EnvelopeKey::with_rotation(
        EnvelopeProfile::new("trace-test").unwrap(),
        "old".into(),
        [3; 32],
        vec![],
        None,
    )
    .unwrap();
    let rotated = EnvelopeKey::with_rotation(
        EnvelopeProfile::new("trace-test").unwrap(),
        "new".into(),
        [4; 32],
        vec![("old".into(), [3; 32])],
        None,
    )
    .unwrap();
    let context = JobContext {
        tenant_id: Uuid::new_v4(),
        resource_id: Uuid::new_v4(),
        operation_id: Uuid::new_v4(),
        revision: 1,
    };
    let payload = TracedJob::with_trace(
        Payload {
            context,
            secret: "private-trace-canary".into(),
        },
        TraceCarrier::try_from(PARENT.to_owned()).unwrap(),
    );
    let ciphertext = seal_job(&key, "traced-job-v2", &payload).unwrap();
    assert!(!ciphertext.contains("private-trace-canary"));
    assert!(!ciphertext.contains(&PARENT[3..35]));
    assert!(!format!("{payload:?}").contains("private-trace-canary"));
    let audit = Audit {
        calls: AtomicUsize::new(0),
        fail: false,
    };
    let denied = Audit {
        calls: AtomicUsize::new(0),
        fail: true,
    };
    let calls = AtomicUsize::new(0);
    let access = JobAccess {
        context,
        purpose: "traced-job-v2",
        actor: "service:worker",
    };
    let handler = |job_context, payload: zeroize::Zeroizing<TracedJob<Payload>>| {
        calls.fetch_add(1, Ordering::SeqCst);
        async move {
            assert_eq!(job_context, context);
            assert_eq!(payload.secret, "private-trace-canary");
            assert_eq!(
                Context::current().span().span_context().trace_id(),
                TraceId::from_hex(&PARENT[3..35]).unwrap()
            );
            Ok::<(), ProcessorError>(())
        }
    };
    assert!(
        process_traced_audited(
            &rotated,
            Uuid::new_v4(),
            access,
            &ciphertext,
            Duration::from_secs(1),
            &audit,
            &handler
        )
        .await
        .is_err()
    );
    assert_eq!(audit.calls.load(Ordering::SeqCst), 0);
    let altered = JobAccess {
        context: JobContext {
            resource_id: Uuid::new_v4(),
            ..context
        },
        ..access
    };
    assert!(
        process_traced_audited(
            &rotated,
            context.tenant_id,
            altered,
            &ciphertext,
            Duration::from_secs(1),
            &audit,
            &handler
        )
        .await
        .is_err()
    );
    assert_eq!(audit.calls.load(Ordering::SeqCst), 0);

    assert!(
        process_traced_audited(
            &rotated,
            context.tenant_id,
            access,
            "v3.invalid",
            Duration::from_secs(1),
            &audit,
            &handler
        )
        .await
        .is_err()
    );
    assert_eq!(audit.calls.load(Ordering::SeqCst), 0);
    assert!(
        process_traced_audited(
            &rotated,
            context.tenant_id,
            access,
            &ciphertext,
            Duration::from_secs(1),
            &denied,
            &handler
        )
        .await
        .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    process_traced_audited(
        &rotated,
        context.tenant_id,
        access,
        &ciphertext,
        Duration::from_secs(1),
        &audit,
        &handler,
    )
    .await
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    assert_eq!(
        process_traced_audited(
            &rotated,
            context.tenant_id,
            access,
            &ciphertext,
            Duration::from_millis(1),
            &NeverAck,
            &handler
        )
        .await,
        Err(ProcessorError::Timeout)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    provider.force_flush().unwrap();
    let finished = spans.get_finished_spans().unwrap();
    let imported: Vec<_> = finished
        .iter()
        .filter(|span| span.span_context.trace_id() == TraceId::from_hex(&PARENT[3..35]).unwrap())
        .collect();
    assert_eq!(imported.len(), 1);
    assert_eq!(imported[0].name, "worker.process");
    assert_eq!(imported[0].parent_span_id.to_string(), &PARENT[36..52]);
    assert!(!format!("{finished:?}").contains("private-trace-canary"));
    // Sanitized permanent and retryable outcomes must retain their queue policy.
    for expected in [
        ProcessorError::InvalidEnvelope,
        ProcessorError::Configuration,
        ProcessorError::Failed,
    ] {
        let outcome = process_traced_audited::<Payload, _, _>(
            &rotated,
            context.tenant_id,
            access,
            &ciphertext,
            Duration::from_secs(1),
            &audit,
            |_, _| async { Err(expected) },
        )
        .await;
        assert_eq!(outcome, Err(expected));
    }

    assert!(
        TraceCarrier::try_from(
            "00-00000000000000000000000000000000-2222222222222222-01".to_owned()
        )
        .is_err()
    );
    assert!(TraceCarrier::try_from(format!("{PARENT},private-baggage")).is_err());
    #[cfg(feature = "schema")]
    {
        let schema = serde_json::to_value(schemars::schema_for!(TraceCarrier)).unwrap();
        let validator =
            forklaunch_core::contract::schema_validation::compile_checked(&schema).unwrap();
        assert!(validator(&serde_json::json!(PARENT)).is_ok());
        assert!(
            validator(&serde_json::json!(
                "00-00000000000000000000000000000000-2222222222222222-01"
            ))
            .is_err()
        );
    }
}
