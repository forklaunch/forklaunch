use crate::encrypted::ProcessorError;
use forklaunch_core::encryption::EnvelopeKey;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

/// Public routing metadata, checked against the authenticated inner payload.
/// Revision is a positive application version, not the queue's retry counter.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobContext {
    pub tenant_id: Uuid,
    pub resource_id: Uuid,
    pub operation_id: Uuid,
    #[cfg_attr(
        feature = "schema",
        schemars(range(min = 1, max = 9_007_199_254_740_991i64))
    )]
    pub revision: i64,
}

impl JobContext {
    fn validate(self) -> Result<(), ProcessorError> {
        if self.tenant_id.is_nil()
            || self.resource_id.is_nil()
            || self.operation_id.is_nil()
            || !(1..=9_007_199_254_740_991).contains(&self.revision)
        {
            return Err(ProcessorError::InvalidEnvelope);
        }
        Ok(())
    }
}

/// Application payloads expose their authenticated identity and wipe sensitive
/// fields on drop through `Zeroizing`. Implementations must not log plaintext.
pub trait JobPayload: Zeroize {
    fn context(&self) -> JobContext;
}

/// Serialize into a wiped temporary buffer, then authenticate the tenant,
/// operation and purpose. The resource and revision are inside the ciphertext.
/// # Errors
/// Rejects invalid routing context, serialization or payload-size failures,
/// and encryption failures without exposing payload contents.
pub fn seal_job<T: JobPayload + Serialize>(
    key: &EnvelopeKey,
    purpose: &str,
    payload: &T,
) -> Result<String, ProcessorError> {
    let context = payload.context();
    context.validate()?;
    let mut bytes = Zeroizing::new(Vec::new());
    serde_json::to_writer(BoundedPayload(&mut bytes), payload)
        .map_err(|_| ProcessorError::InvalidEnvelope)?;
    key.seal(context.tenant_id, context.operation_id, purpose, &bytes)
        .map_err(|_| ProcessorError::InvalidEnvelope)
}

// Stop serialization at the envelope's plaintext limit rather than allocating
// an arbitrarily large serialized payload and rejecting it only afterwards.
struct BoundedPayload<'a>(&'a mut Vec<u8>);
impl std::io::Write for BoundedPayload<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > 1_048_576usize.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other("worker payload too large"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Reject tampering and mismatched routing metadata before calling application
/// code. Both the plaintext bytes and decoded sensitive fields are wiped on drop.
/// Use `deny_unknown_fields` on payload structs for a closed wire contract.
/// # Errors
/// Rejects invalid routing context, unauthenticated ciphertext, invalid
/// payload encoding, or mismatched authenticated identity.
pub fn decode_job<T: JobPayload + DeserializeOwned>(
    key: &EnvelopeKey,
    context: JobContext,
    purpose: &str,
    ciphertext: &str,
) -> Result<Zeroizing<T>, ProcessorError> {
    context.validate()?;
    let bytes = key
        .open(context.tenant_id, context.operation_id, purpose, ciphertext)
        .map_err(|_| ProcessorError::InvalidEnvelope)?;
    let payload: Zeroizing<T> = Zeroizing::new(
        serde_json::from_slice(&bytes).map_err(|_| ProcessorError::InvalidEnvelope)?,
    );
    if payload.context() != context {
        return Err(ProcessorError::InvalidEnvelope);
    }
    Ok(payload)
}

#[cfg(test)]
mod tests {
    #[test]
    fn revision_is_a_positive_javascript_safe_integer() {
        let context = JobContext {
            tenant_id: Uuid::new_v4(),
            resource_id: Uuid::new_v4(),
            operation_id: Uuid::new_v4(),
            revision: 9_007_199_254_740_991,
        };
        assert!(context.validate().is_ok());
        for revision in [0, -1, 9_007_199_254_740_992, i64::MAX] {
            assert_eq!(
                JobContext {
                    revision,
                    ..context
                }
                .validate(),
                Err(ProcessorError::InvalidEnvelope)
            );
        }
    }
    use super::*;
    use forklaunch_core::encryption::EnvelopeProfile;
    use serde::Deserialize;

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Payload {
        tenant: Uuid,
        resource: Uuid,
        operation: Uuid,
        revision: i64,
        secret: String,
    }
    impl Zeroize for Payload {
        fn zeroize(&mut self) {
            self.secret.zeroize();
        }
    }
    impl JobPayload for Payload {
        fn context(&self) -> JobContext {
            JobContext {
                tenant_id: self.tenant,
                resource_id: self.resource,
                operation_id: self.operation,
                revision: self.revision,
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn processor_rejects_before_handler_and_bounds_failure_output() {
        use std::time::Duration;
        let key = EnvelopeKey::new(EnvelopeProfile::new("worker-test").unwrap(), [7; 32]);
        let payload = Zeroizing::new(Payload {
            tenant: Uuid::new_v4(),
            resource: Uuid::new_v4(),
            operation: Uuid::new_v4(),
            revision: 1,
            secret: "private note text".into(),
        });
        let context = payload.context();
        let sealed = seal_job(&key, "job-v1", &*payload).unwrap();
        let invalid = crate::encrypted::process(
            &key,
            context,
            "wrong-purpose",
            &sealed,
            Duration::from_secs(1),
            |_, _: Zeroizing<Payload>| async {
                panic!("unauthenticated handler invoked");
                #[allow(unreachable_code)]
                Ok::<(), String>(())
            },
        )
        .await;
        assert_eq!(invalid, Err(ProcessorError::InvalidEnvelope));
        let failure = crate::encrypted::process(
            &key,
            context,
            "job-v1",
            &sealed,
            Duration::from_secs(1),
            |_, _: Zeroizing<Payload>| async { Err::<(), _>("private response and secret token") },
        )
        .await
        .unwrap_err();
        assert_eq!(failure.to_string(), "worker processing failed");
        let timeout = crate::encrypted::process(
            &key,
            context,
            "job-v1",
            &sealed,
            Duration::from_secs(1),
            |_, _: Zeroizing<Payload>| std::future::pending::<Result<(), String>>(),
        )
        .await;
        assert_eq!(timeout, Err(ProcessorError::Timeout));
        let success = crate::encrypted::process(
            &key,
            context,
            "job-v1",
            &sealed,
            Duration::from_secs(1),
            |actual, job: Zeroizing<Payload>| async move {
                assert_eq!(actual, job.context());
                Ok::<(), String>(())
            },
        )
        .await;
        assert_eq!(success, Ok(()));
    }

    #[test]
    fn rejects_tampering_and_every_context_mismatch() {
        let key = EnvelopeKey::new(EnvelopeProfile::new("worker-test").unwrap(), [7; 32]);
        let payload = Zeroizing::new(Payload {
            tenant: Uuid::new_v4(),
            resource: Uuid::new_v4(),
            operation: Uuid::new_v4(),
            revision: 1,
            secret: "private note text".into(),
        });
        let context = payload.context();
        let sealed = seal_job(&key, "job-v1", &*payload).unwrap();
        assert!(!sealed.contains("private"));
        assert_eq!(
            decode_job::<Payload>(&key, context, "job-v1", &sealed)
                .unwrap()
                .secret,
            payload.secret
        );
        for modified in [
            JobContext {
                tenant_id: Uuid::new_v4(),
                ..context
            },
            JobContext {
                operation_id: Uuid::new_v4(),
                ..context
            },
            JobContext {
                resource_id: Uuid::new_v4(),
                ..context
            },
            JobContext {
                revision: 2,
                ..context
            },
            JobContext {
                revision: 0,
                ..context
            },
        ] {
            assert!(decode_job::<Payload>(&key, modified, "job-v1", &sealed).is_err());
        }
        assert!(decode_job::<Payload>(&key, context, "other-purpose", &sealed).is_err());
        let (prefix, encrypted) = sealed.rsplit_once('.').unwrap();
        let mut tampered = encrypted.as_bytes().to_vec();
        tampered[0] = if tampered[0] == b'A' { b'B' } else { b'A' };
        let tampered = format!("{prefix}.{}", String::from_utf8(tampered).unwrap());
        assert!(decode_job::<Payload>(&key, context, "job-v1", &tampered).is_err());
    }
}
