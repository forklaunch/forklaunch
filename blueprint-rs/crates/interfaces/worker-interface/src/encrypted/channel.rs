//! Broker-independent encrypted event boundaries. Brokers store only `SealedEvent`.
use crate::encrypted::{JobContext, JobPayload, ProcessorError, decode_job, seal_job};
use forklaunch_core::encryption::EnvelopeKey;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{fmt, future::Future, marker::PhantomData, time::Duration};
use uuid::Uuid;
use zeroize::Zeroizing;

/// Transport representation. Routing identifiers are public; application data is
/// encrypted. Deserialization is not authentication: call the channel boundary
/// before using context or invoking a handler. Retry/DLQ paths retain this value.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SealedEvent {
    channel: String,
    context: JobContext,
    ciphertext: String,
}

impl fmt::Debug for SealedEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Even malformed broker input must not become a diagnostic data leak.
        f.write_str("SealedEvent([redacted])")
    }
}

impl SealedEvent {
    /// Reconstitute an envelope from a broker's separately stored columns.
    /// This is untrusted input, equivalent to deserializing `SealedEvent`; opening
    /// the channel still authenticates every field before application use.
    pub fn from_parts(channel: impl Into<String>, context: JobContext, ciphertext: String) -> Self {
        Self {
            channel: channel.into(),
            context,
            ciphertext,
        }
    }

    /// Ciphertext for adapters with a dedicated encrypted-payload column.
    #[must_use]
    pub fn ciphertext(&self) -> &str {
        &self.ciphertext
    }

    /// Untrusted routing metadata until authenticated by `EncryptedChannel`.
    #[must_use]
    pub fn context(&self) -> JobContext {
        self.context
    }
}

/// A versioned event contract whose name is authenticated as encryption purpose.
/// Use distinct names for unrelated schemas. This is not a replay ledger: the
/// application still checks operation IDs/revisions in its transaction. Tenant
/// selection must come from trusted configuration/authentication, not this event.
pub struct EncryptedChannel<T> {
    name: String,
    payload: PhantomData<fn() -> T>,
}

impl<T> EncryptedChannel<T> {
    /// # Errors
    /// Returns a configuration error unless the channel name is 1–64 ASCII
    /// alphanumeric or hyphen characters.
    pub fn new(name: impl Into<String>) -> Result<Self, ProcessorError> {
        let name = name.into();
        if name.is_empty()
            || name.len() > 64
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(ProcessorError::Configuration);
        }
        Ok(Self {
            name,
            payload: PhantomData,
        })
    }
}

impl<T: JobPayload + Serialize> EncryptedChannel<T> {
    /// Seal before calling any queue, stream, outbox or retry adapter.
    /// # Errors
    /// Rejects a tenant mismatch, invalid context, serialization failure or
    /// an envelope that cannot be sealed within the payload limits.
    pub fn seal(
        &self,
        key: &EnvelopeKey,
        tenant: Uuid,
        payload: &T,
    ) -> Result<SealedEvent, ProcessorError> {
        let context = payload.context();
        if context.tenant_id != tenant {
            return Err(ProcessorError::InvalidEnvelope);
        }
        Ok(SealedEvent {
            channel: self.name.clone(),
            context,
            ciphertext: seal_job(key, &self.name, payload)?,
        })
    }
}

impl<T: JobPayload + DeserializeOwned> EncryptedChannel<T> {
    /// # Errors
    /// Rejects a channel or tenant mismatch, invalid ciphertext, or decoded
    /// payload context that differs from the routing metadata.
    pub fn open(
        &self,
        key: &EnvelopeKey,
        tenant: Uuid,
        event: &SealedEvent,
    ) -> Result<Zeroizing<T>, ProcessorError> {
        if event.channel != self.name || event.context.tenant_id != tenant {
            return Err(ProcessorError::InvalidEnvelope);
        }
        decode_job(key, event.context, &self.name, &event.ciphertext)
    }
    /// Decode a classified message only after durable metadata-only access audit.
    /// # Errors
    /// Rejects invalid channel, tenant, actor or payload context, and returns
    /// a fixed failure if the audit sink does not acknowledge access.
    pub async fn open_audited(
        &self,
        key: &EnvelopeKey,
        tenant: Uuid,
        event: &SealedEvent,
        actor: &str,
        audit: &dyn crate::encrypted::JobAccessAudit,
    ) -> Result<Zeroizing<T>, ProcessorError> {
        if tenant != event.context.tenant_id || event.channel != self.name {
            return Err(ProcessorError::InvalidEnvelope);
        }
        crate::encrypted::decode_job_audited(
            key,
            tenant,
            event.context,
            &self.name,
            &event.ciphertext,
            actor,
            audit,
        )
        .await
    }

    /// Only unit success or fixed errors leave this boundary. Never persist a
    /// decrypted handler result or arbitrary error text to the broker.
    /// # Errors
    /// Returns a configuration error for an invalid timeout, an envelope error
    /// for rejected input, a fixed handler failure, or a timeout.
    pub async fn process<F, Fut, E>(
        &self,
        key: &EnvelopeKey,
        tenant: Uuid,
        event: &SealedEvent,
        timeout: Duration,
        handler: F,
    ) -> Result<(), ProcessorError>
    where
        F: FnOnce(JobContext, Zeroizing<T>) -> Fut,
        Fut: Future<Output = Result<(), E>>,
    {
        crate::encrypted::process_attempt(timeout, async {
            let payload = self.open(key, tenant, event)?;
            handler(event.context, payload)
                .await
                .map_err(|_| ProcessorError::Failed)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forklaunch_core::encryption::EnvelopeProfile;
    use zeroize::Zeroize;

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Event {
        context: JobContext,
        secret: String,
    }
    impl Zeroize for Event {
        fn zeroize(&mut self) {
            self.secret.zeroize();
        }
    }
    impl JobPayload for Event {
        fn context(&self) -> JobContext {
            self.context
        }
    }
    fn key(id: &str, value: u8, previous: Vec<(String, [u8; 32])>) -> EnvelopeKey {
        EnvelopeKey::with_rotation(
            EnvelopeProfile::new("events").unwrap(),
            id.into(),
            [value; 32],
            previous,
            None,
        )
        .unwrap()
    }
    fn payload() -> Event {
        Event {
            context: JobContext {
                tenant_id: Uuid::new_v4(),
                resource_id: Uuid::new_v4(),
                operation_id: Uuid::new_v4(),
                revision: 1,
            },
            secret: "private stream canary".into(),
        }
    }
    #[test]
    fn rotation_retries_and_storage_never_require_plaintext() {
        let channel = EncryptedChannel::<Event>::new("notes-insights-v1").unwrap();
        let payload = Zeroizing::new(payload());
        let tenant = payload.context.tenant_id;
        let old = key("old", 1, vec![]);
        let rotated = key("new", 2, vec![("old".into(), [1; 32])]);
        let sealed = channel.seal(&old, tenant, &payload).unwrap();
        let wire = serde_json::to_string(&sealed).unwrap();
        assert!(!wire.contains(&payload.secret));
        let redelivered: SealedEvent = serde_json::from_str(&wire).unwrap();
        assert_eq!(
            channel.open(&rotated, tenant, &redelivered).unwrap().secret,
            payload.secret
        );
        assert!(
            channel
                .open(&key("new", 2, vec![]), tenant, &redelivered)
                .is_err()
        );
        let next = channel.seal(&rotated, tenant, &payload).unwrap();
        assert!(next.ciphertext.starts_with("v3.new."));
        assert!(channel.open(&old, tenant, &next).is_err());
    }
    #[tokio::test]
    async fn reject_substitution_before_handler_and_sanitize_failures() {
        let channel = EncryptedChannel::<Event>::new("notes-insights-v1").unwrap();
        let other = EncryptedChannel::<Event>::new("other-stream-v1").unwrap();
        let key = key("current", 3, vec![]);
        let payload = Zeroizing::new(payload());
        let tenant = payload.context.tenant_id;
        let event = channel.seal(&key, tenant, &payload).unwrap();
        assert!(channel.seal(&key, Uuid::new_v4(), &payload).is_err());
        assert!(channel.open(&key, Uuid::new_v4(), &event).is_err());
        assert!(other.open(&key, tenant, &event).is_err());
        let mut forged = event.clone();
        forged.channel = "other-stream-v1".into();
        assert!(other.open(&key, tenant, &forged).is_err());
        forged = event.clone();
        forged.context.revision += 1;
        let result = channel
            .process(
                &key,
                tenant,
                &forged,
                Duration::from_secs(1),
                |_, _| async {
                    panic!("forged event reached handler");
                    #[allow(unreachable_code)]
                    Ok::<(), String>(())
                },
            )
            .await;
        assert_eq!(result, Err(ProcessorError::InvalidEnvelope));
        let result = channel
            .process(
                &key,
                tenant,
                &event,
                Duration::from_secs(1),
                |_, job| async move { Err::<(), _>(job.secret.clone()) },
            )
            .await;
        assert_eq!(result, Err(ProcessorError::Failed));
        assert!(!format!("{result:?} {event:?}").contains(&payload.secret));
        forged.ciphertext = payload.secret.clone();
        assert!(!format!("{forged:?}").contains(&payload.secret));
    }
}
