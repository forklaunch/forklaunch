//! Shared typed input and authenticated event contract for publishers/processors.
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::config;
use forklaunch_core::encryption::{EnvelopeError, EnvelopeKey, EnvelopeProfile};
use forklaunch_worker_interface::{EncryptedChannel, SealedEvent};
use forklaunch_worker_interface::Job;
use uuid::Uuid;
use zeroize::Zeroizing;

pub use super::event_contract::{WorkEvent, WorkInput};
use crate::WORKER_NAME;
use forklaunch_worker_interface::{BrokerTransportPolicy, JobContext, ProcessorError};

/// The producer always authenticates and encrypts before returning transport data.
pub fn seal_work(
    key: &EnvelopeKey,
    tenant: Uuid,
    queue: &str,
    operation: Uuid,
    input: WorkInput,
) -> Result<SealedEvent, ProcessorError> {
    let event = Zeroizing::new(WorkEvent {
        context: JobContext {
            tenant_id: tenant,
            resource_id: input.resource_id,
            operation_id: operation,
            revision: input.revision,
        },
        queue: queue.into(),
        input,
    });
    EncryptedChannel::new("work-v1")?.seal(key, tenant, &*event)
}

fn rejected() -> BlueprintError {
    BlueprintError::new(
        "worker_envelope_rejected",
        "Worker encrypted event unavailable",
    )
}
struct EventBoundary {
    channel: EncryptedChannel<WorkEvent>,
    key: EnvelopeKey,
    tenant: Uuid,
    queue: String,
}
/// Missing settings are named; every other failure stays opaque.
fn boundary() -> BlueprintResult<EventBoundary> {
    let tenant: Uuid = config::required("TENANT_ID")?;
    if tenant.is_nil() {
        return Err(rejected());
    }
    let profile = EnvelopeProfile::new(WORKER_NAME).map_err(|_| rejected())?;
    Ok(EventBoundary {
        key: EnvelopeKey::from_env(profile).map_err(|error| match error {
            EnvelopeError::Configuration(error) => error.into(),
            _ => rejected(),
        })?,
        channel: EncryptedChannel::new("work-v1").map_err(|_| rejected())?,
        tenant,
        queue: config::required("WORKER_QUEUE")?,
    })
}
/// Transport admission is separate from payload encryption. Remote brokers
/// require verified TLS unless the required `WORKER_ALLOW_PLAINTEXT_DEVELOPMENT`
/// explicitly selects local development.
pub fn transport_policy() -> BlueprintResult<BrokerTransportPolicy> {
    let allow: String = config::required("WORKER_ALLOW_PLAINTEXT_DEVELOPMENT")?;
    match allow.as_str() {
        "true" => Ok(BrokerTransportPolicy::PlaintextDevelopment),
        "false" => Ok(BrokerTransportPolicy::default()),
        _ => Err(rejected()),
    }
}

/// Fail startup before pulling work when key/tenant configuration is missing.
pub fn validate_configuration() -> BlueprintResult<()> {
    boundary().map(|_| ())
}
/// Encrypt typed input before any queue backend sees it.
pub fn seal(id: &str, input: &str) -> BlueprintResult<String> {
    boundary()?.seal(id, input)
}
/// Authenticate the envelope before exposing plaintext to application code.
pub fn open(job: &Job) -> BlueprintResult<Zeroizing<WorkEvent>> {
    boundary()?.open(job)
}

impl EventBoundary {
    fn seal(&self, id: &str, input: &str) -> BlueprintResult<String> {
        if input.len() > 1_048_576 {
            return Err(rejected());
        }
        let operation_id = id.parse().map_err(|_| rejected())?;
        let input: WorkInput = serde_json::from_str(input).map_err(|_| rejected())?;
        let sealed = seal_work(
            &self.key,
            self.tenant,
            &self.queue,
            operation_id,
            input,
        )
        .map_err(|_| rejected())?;
        serde_json::to_string(&sealed).map_err(|_| rejected())
    }
    fn open(&self, job: &Job) -> BlueprintResult<Zeroizing<WorkEvent>> {
        if job.payload_json.len() > 1_500_000 || job.queue != self.queue {
            return Err(rejected());
        }
        let event: SealedEvent = serde_json::from_str(&job.payload_json).map_err(|_| rejected())?;
        if job.id.parse::<Uuid>().map_err(|_| rejected())? != event.context().operation_id {
            return Err(rejected());
        }
        let payload = self
            .channel
            .open(&self.key, self.tenant, &event)
            .map_err(|_| rejected())?;
        if payload.queue != self.queue
            || payload.input.resource_id != payload.context.resource_id
            || payload.input.revision != payload.context.revision
        {
            return Err(rejected());
        }
        Ok(payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn boundary(queue: &str, tenant: Uuid) -> EventBoundary {
        EventBoundary {
            channel: EncryptedChannel::new("work-v1").unwrap(),
            key: EnvelopeKey::new(EnvelopeProfile::new("worker-test").unwrap(), [7; 32]),
            tenant,
            queue: queue.into(),
        }
    }
    #[test]
    fn producer_rejects_revisions_outside_javascript_safe_range() {
        let producer = boundary("jobs", Uuid::new_v4());
        let operation = Uuid::new_v4().to_string();
        for revision in [0, -1, 9_007_199_254_740_992, i64::MAX] {
            let input = serde_json::json!({"resourceId": Uuid::new_v4(), "revision": revision, "message": "private"}).to_string();
            assert!(producer.seal(&operation, &input).is_err());
        }
        let input = serde_json::json!({"resourceId": Uuid::new_v4(), "revision": 9_007_199_254_740_991i64, "message": "private"}).to_string();
        let sealed = producer.seal(&operation, &input).unwrap();
        let job = Job::new(&operation, "jobs", &sealed);
        assert_eq!(producer.open(&job).unwrap().input.revision, 9_007_199_254_740_991);
    }

    #[test]
    fn stored_events_hide_plaintext_and_bind_queue_tenant_and_operation() {
        let tenant = Uuid::new_v4();
        let producer = boundary("jobs", tenant);
        let operation = Uuid::new_v4().to_string();
        let input = serde_json::json!({"resourceId":Uuid::new_v4(),"revision":1,"message":"private marker"}).to_string();
        let sealed = producer.seal(&operation, &input).unwrap();
        assert!(!sealed.contains("private marker"));
        let job = Job::new(&operation, "jobs", &sealed);
        assert_eq!(producer.open(&job).unwrap().input.message, "private marker");
        assert!(boundary("jobs", Uuid::new_v4()).open(&job).is_err());
        assert!(
            boundary("other", tenant)
                .open(&Job::new(&operation, "other", &sealed))
                .is_err()
        );
        assert!(
            producer
                .open(&Job::new(Uuid::new_v4().to_string(), "jobs", &sealed))
                .is_err()
        );
        assert!(
            producer
                .open(&Job::new(&operation, "jobs", &input))
                .is_err()
        );
        let mut tampered: serde_json::Value = serde_json::from_str(&sealed).unwrap();
        tampered["context"]["revision"] = 2.into();
        assert!(
            producer
                .open(&Job::new(&operation, "jobs", tampered.to_string()))
                .is_err()
        );
    }
}
