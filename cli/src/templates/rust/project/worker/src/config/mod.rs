use crate::error::{Result, WorkerError};
use forklaunch_core::config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerConfig {
    pub name: String,
    pub worker_type: String,
    pub queue_name: String,
    pub batch_size: usize,
}

impl WorkerConfig {
    /// Reads `WORKER_TYPE`, `WORKER_QUEUE` and `WORKER_BATCH_SIZE`; each is
    /// required at startup and a missing or malformed one is an error naming it.
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            name: "{{{name}}}".to_string(),
            worker_type: config::required("WORKER_TYPE")?,
            queue_name: config::required("WORKER_QUEUE")?,
            batch_size: config::required("WORKER_BATCH_SIZE")?,
        })
    }

    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(WorkerError::InvalidConfig("worker name cannot be empty".to_string()));
        }
        if self.batch_size == 0 {
            return Err(WorkerError::InvalidConfig(
                "batch size must be greater than zero".to_string(),
            ));
        }
        Ok(())
    }
}

/// A fixed configuration for tests; processes use [`WorkerConfig::from_env`].
impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            name: "{{{name}}}".to_string(),
            worker_type: "database".to_string(),
            queue_name: "{{{name}}}".to_string(),
            batch_size: 1,
        }
    }
}
