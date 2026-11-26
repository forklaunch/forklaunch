//! Explicit versioned payload wrapper for encrypted cross-process trace continuation.
//! Trace metadata is inside the ciphertext, never read from broker routing headers.
use super::{JobContext, JobPayload, ProcessorError};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

/// A bounded W3C version-00 traceparent. Baggage and vendor tracestate are excluded.
#[derive(Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TraceCarrier(String);
#[cfg(feature = "schema")]
impl schemars::JsonSchema for TraceCarrier {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TraceCarrier".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        serde_json::from_value(serde_json::json!({
            "type":"string", "minLength":55, "maxLength":55,
            "pattern":"^00-[0-9a-f]{32}-[0-9a-f]{16}-[0-9a-f]{2}$",
            "not":{"anyOf":[
                {"pattern":"^00-00000000000000000000000000000000-"},
                {"pattern":"^00-[0-9a-f]{32}-0000000000000000-"}
            ]}
        }))
        .expect("static traceparent schema is valid")
    }
}
impl std::fmt::Debug for TraceCarrier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TraceCarrier([redacted])")
    }
}
impl Zeroize for TraceCarrier {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}
impl TryFrom<String> for TraceCarrier {
    type Error = ProcessorError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let valid_hex = |s: &str| {
            s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if value.len() != 55
            || !value.is_ascii()
            || &value[..3] != "00-"
            || value.as_bytes()[35] != b'-'
            || value.as_bytes()[52] != b'-'
            || !valid_hex(&value[3..35])
            || !valid_hex(&value[36..52])
            || !valid_hex(&value[53..55])
            || value[3..35].bytes().all(|b| b == b'0')
            || value[36..52].bytes().all(|b| b == b'0')
        {
            return Err(ProcessorError::InvalidEnvelope);
        }
        Ok(Self(value))
    }
}
impl From<TraceCarrier> for String {
    fn from(value: TraceCarrier) -> Self {
        value.0
    }
}
impl TraceCarrier {
    /// Capture only the current trace identity, never baggage, payload or tenant data.
    #[cfg(feature = "telemetry")]
    #[must_use]
    pub fn capture() -> Option<Self> {
        use opentelemetry::trace::TraceContextExt;
        let context = opentelemetry::Context::current();
        let span = context.span();
        let span = span.span_context();
        span.is_valid().then(|| {
            Self(format!(
                "00-{}-{}-{:02x}",
                span.trace_id(),
                span.span_id(),
                span.trace_flags().to_u8()
            ))
        })
    }
    #[cfg(feature = "telemetry")]
    fn authenticated_parent(&self) -> opentelemetry::Context {
        use opentelemetry::trace::{
            SpanContext, SpanId, TraceContextExt, TraceFlags, TraceId, TraceState,
        };
        // Construction and deserialization validate these fixed-width components.
        let trace = TraceId::from_hex(&self.0[3..35]).expect("validated trace id");
        let span = SpanId::from_hex(&self.0[36..52]).expect("validated span id");
        let flags = u8::from_str_radix(&self.0[53..55], 16).expect("validated trace flags");
        opentelemetry::Context::new().with_remote_span_context(SpanContext::new(
            trace,
            span,
            TraceFlags::new(flags),
            true,
            TraceState::default(),
        ))
    }
}

/// Explicit encrypted wire version: publish this schema under a new channel/purpose.
/// Existing raw payload readers are not silently upgraded or weakened.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TracedJob<T> {
    payload: T,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    trace: Option<TraceCarrier>,
}
impl<T> std::fmt::Debug for TracedJob<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TracedJob([redacted])")
    }
}
impl<T: Zeroize> Zeroize for TracedJob<T> {
    fn zeroize(&mut self) {
        self.payload.zeroize();
        self.trace.zeroize();
    }
}
impl<T: JobPayload> JobPayload for TracedJob<T> {
    fn context(&self) -> JobContext {
        self.payload.context()
    }
}
impl<T> std::ops::Deref for TracedJob<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.payload
    }
}
impl<T> TracedJob<T> {
    /// Wrap a payload, capturing an active parent when telemetry is enabled.
    #[must_use]
    pub fn new(payload: T) -> Self {
        #[cfg(feature = "telemetry")]
        let trace = TraceCarrier::capture();
        #[cfg(not(feature = "telemetry"))]
        let trace = None;
        Self { payload, trace }
    }
    /// Supply a validated parent explicitly at a trusted producer boundary.
    #[must_use]
    pub const fn with_trace(payload: T, trace: TraceCarrier) -> Self {
        Self {
            payload,
            trace: Some(trace),
        }
    }
    #[must_use]
    pub const fn payload(&self) -> &T {
        &self.payload
    }
    #[cfg(feature = "telemetry")]
    fn authenticated_parent(&self) -> opentelemetry::Context {
        self.trace.as_ref().map_or_else(
            opentelemetry::Context::new,
            TraceCarrier::authenticated_parent,
        )
    }
}

/// Authenticate and acknowledge classified access before importing a trace parent.
/// Timeout covers audit plus handler; failed audit never activates the remote trace.
/// # Errors
/// Rejects timeout/configuration, tenant/context/ciphertext/schema, audit, or handler failures.
#[cfg(feature = "telemetry")]
pub async fn process_traced_audited<T, F, Fut>(
    key: &forklaunch_core::encryption::EnvelopeKey,
    expected_tenant: uuid::Uuid,
    access: super::JobAccess<'_>,
    ciphertext: &str,
    timeout: std::time::Duration,
    audit: &dyn super::JobAccessAudit,
    handler: F,
) -> Result<(), ProcessorError>
where
    T: JobPayload + serde::de::DeserializeOwned,
    F: FnOnce(JobContext, zeroize::Zeroizing<TracedJob<T>>) -> Fut,
    Fut: std::future::Future<Output = Result<(), ProcessorError>>,
{
    if timeout.is_zero() || timeout > std::time::Duration::from_secs(300) {
        return super::process_attempt(timeout, async { Err(ProcessorError::Configuration) }).await;
    }
    let started = tokio::time::Instant::now();
    let decoded = tokio::time::timeout(
        timeout,
        super::decode_job_audited::<TracedJob<T>>(
            key,
            expected_tenant,
            access.context,
            access.purpose,
            ciphertext,
            access.actor,
            audit,
        ),
    )
    .await;
    let payload = match decoded {
        Ok(Ok(payload)) => payload,
        Ok(Err(error)) => return super::process_attempt(timeout, async { Err(error) }).await,
        Err(_) => {
            return super::process_attempt(timeout, async { Err(ProcessorError::Timeout) }).await;
        }
    };
    let parent = payload.authenticated_parent();
    let remaining = timeout.saturating_sub(started.elapsed());
    if remaining.is_zero() {
        return super::telemetry::observe_with_parent(parent, async {
            Err(ProcessorError::Timeout)
        })
        .await;
    }
    super::telemetry::observe_with_parent(parent, async move {
        tokio::time::timeout(remaining, handler(access.context, payload))
            .await
            .map_err(|_| ProcessorError::Timeout)?
    })
    .await
}

#[cfg(all(test, feature = "telemetry"))]
mod capture_tests {
    use super::*;
    use opentelemetry::{
        Context,
        baggage::BaggageExt,
        trace::{SpanContext, SpanId, TraceContextExt, TraceFlags, TraceId, TraceState},
    };

    #[test]
    fn producer_captures_only_valid_current_span_without_baggage_or_tracestate() {
        let context = Context::new()
            .with_remote_span_context(SpanContext::new(
                TraceId::from_hex("0123456789abcdef0123456789abcdef").unwrap(),
                SpanId::from_hex("0123456789abcdef").unwrap(),
                TraceFlags::SAMPLED,
                true,
                TraceState::from_key_value([("vendor", "private-state")]).unwrap(),
            ))
            .with_baggage([opentelemetry::KeyValue::new("user", "private-baggage")]);
        let encoded = {
            let _scope = context.attach();
            serde_json::to_value(TracedJob::new(())).unwrap()
        };
        assert_eq!(
            encoded["trace"],
            "00-0123456789abcdef0123456789abcdef-0123456789abcdef-01"
        );
        assert!(!encoded.to_string().contains("private"));
        let _scope = Context::new().attach();
        assert!(
            serde_json::to_value(TracedJob::new(()))
                .unwrap()
                .get("trace")
                .is_none()
        );
    }
}
