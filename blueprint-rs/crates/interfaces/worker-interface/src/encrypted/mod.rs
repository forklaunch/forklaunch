//! Secure processing boundaries around native queue runtimes.
//!
//! Encryption authenticates job context; it does not authorize the action or
//! provide exactly-once delivery. Publishers must enforce operation idempotency,
//! current authorization and revision preconditions at their transactional boundary.

mod access;
mod channel;
mod envelope;
mod error;
mod processor;
mod retry;
#[cfg(feature = "telemetry")]
mod telemetry;
mod trace;
mod transport;
#[cfg(feature = "telemetry")]
pub use trace::process_traced_audited;
pub use trace::{TraceCarrier, TracedJob};

pub use access::{JobAccess, JobAccessAudit, decode_job_audited};
pub use channel::{EncryptedChannel, SealedEvent};
pub use envelope::{JobContext, JobPayload, decode_job, seal_job};
pub use error::ProcessorError;
pub use processor::{process, process_attempt};
pub use retry::RetrySettings;
pub use transport::BrokerTransportPolicy;

#[cfg(feature = "apalis")]
pub mod apalis;

#[cfg(feature = "native")]
pub mod native;
