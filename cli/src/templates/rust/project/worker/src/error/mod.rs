use forklaunch_core::config::ConfigError;
use std::{fmt, error::Error, result};

pub type Result<T> = result::Result<T, WorkerError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerError {
    InvalidConfig(String),
    JobFailed(String),
}

impl fmt::Display for WorkerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(message) => write!(formatter, "invalid config: {message}"),
            Self::JobFailed(message) => write!(formatter, "job failed: {message}"),
        }
    }
}

impl Error for WorkerError {}

impl From<ConfigError> for WorkerError {
    fn from(error: ConfigError) -> Self {
        Self::InvalidConfig(error.to_string())
    }
}
