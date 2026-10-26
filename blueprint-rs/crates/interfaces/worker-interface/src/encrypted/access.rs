//! Optional acknowledged access boundary for classified encrypted job payloads.
//! Audit implementations receive metadata only. Failed acknowledgment never
//! releases decoded plaintext to the application handler.
use crate::encrypted::{JobContext, JobPayload, ProcessorError, decode_job};
use forklaunch_core::encryption::EnvelopeKey;
use serde::de::DeserializeOwned;
use std::{future::Future, pin::Pin};
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug)]
pub struct JobAccess<'a> {
    pub context: JobContext,
    pub purpose: &'a str,
    pub actor: &'a str,
}
/// Implementors must acknowledge durable access records and enforce the trusted
/// service identity. No sensitive payload or handler error is supplied.
pub trait JobAccessAudit: Send + Sync {
    fn record<'a>(
        &'a self,
        access: JobAccess<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ProcessorError>> + Send + 'a>>;
}
/// The expected tenant comes from trusted authorization/routing, never from
/// accepting an unchecked ciphertext's public context as permission.
/// # Errors
/// Rejects invalid tenant, actor, purpose, ciphertext or payload context. Returns
/// a fixed failure if the audit sink does not acknowledge the access record.
pub async fn decode_job_audited<T: JobPayload + DeserializeOwned>(
    key: &EnvelopeKey,
    expected_tenant: Uuid,
    context: JobContext,
    purpose: &str,
    ciphertext: &str,
    actor: &str,
    audit: &dyn JobAccessAudit,
) -> Result<Zeroizing<T>, ProcessorError> {
    if expected_tenant != context.tenant_id
        || actor.is_empty()
        || actor.len() > 256
        || actor.chars().any(char::is_control)
        || purpose.is_empty()
        || purpose.len() > 128
        || purpose.chars().any(char::is_control)
    {
        return Err(ProcessorError::InvalidEnvelope);
    }
    let payload = decode_job(key, context, purpose, ciphertext)?;
    audit
        .record(JobAccess {
            context,
            purpose,
            actor,
        })
        .await
        .map_err(|_| ProcessorError::Failed)?;
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forklaunch_core::encryption::EnvelopeProfile;
    use serde::{Deserialize, Serialize};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use zeroize::Zeroize;
    #[derive(Serialize, Deserialize)]
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
            access: JobAccess<'a>,
        ) -> Pin<Box<dyn Future<Output = Result<(), ProcessorError>> + Send + 'a>> {
            Box::pin(async move {
                assert_eq!(access.actor, "service:insights");
                assert_eq!(access.purpose, "note-job");
                assert!(!format!("{access:?}").contains("private-canary"));
                self.calls.fetch_add(1, Ordering::SeqCst);
                if self.fail {
                    Err(ProcessorError::Configuration)
                } else {
                    Ok(())
                }
            })
        }
    }
    #[tokio::test]
    async fn durable_ack_required_and_invalid_tenant_never_records_access() {
        let key = EnvelopeKey::new(EnvelopeProfile::new("access-test").unwrap(), [4; 32]);
        let context = JobContext {
            tenant_id: Uuid::new_v4(),
            resource_id: Uuid::new_v4(),
            operation_id: Uuid::new_v4(),
            revision: 1,
        };
        let payload = Payload {
            context,
            secret: "private-canary".into(),
        };
        let ciphertext = crate::encrypted::seal_job(&key, "note-job", &payload).unwrap();
        let audit = Audit {
            calls: AtomicUsize::new(0),
            fail: false,
        };
        let read = |tenant| {
            decode_job_audited::<Payload>(
                &key,
                tenant,
                context,
                "note-job",
                &ciphertext,
                "service:insights",
                &audit,
            )
        };
        assert!(matches!(
            read(Uuid::new_v4()).await,
            Err(ProcessorError::InvalidEnvelope)
        ));
        assert_eq!(audit.calls.load(Ordering::SeqCst), 0);
        assert!(matches!(
            decode_job_audited::<Payload>(
                &key,
                context.tenant_id,
                context,
                "note-job",
                "private-invalid-ciphertext",
                "service:insights",
                &audit
            )
            .await,
            Err(ProcessorError::InvalidEnvelope)
        ));
        assert_eq!(audit.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            read(context.tenant_id).await.unwrap().secret,
            "private-canary"
        );
        assert_eq!(audit.calls.load(Ordering::SeqCst), 1);
        let failing = Audit {
            calls: AtomicUsize::new(0),
            fail: true,
        };
        assert!(matches!(
            decode_job_audited::<Payload>(
                &key,
                context.tenant_id,
                context,
                "note-job",
                &ciphertext,
                "service:insights",
                &failing
            )
            .await,
            Err(ProcessorError::Failed)
        ));
        assert_eq!(failing.calls.load(Ordering::SeqCst), 1);
    }
}
