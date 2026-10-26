//! Service/domain errors shared by application modules. Controllers translate
//! these into public endpoint errors; internal messages are not HTTP responses.
use forklaunch_core::config::ConfigError;
use std::{fmt, error::Error, result};

pub type Result<T> = result::Result<T, AppError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    InvalidConfig(String),
    NotFound(String),
    Storage(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(message) => write!(formatter, "invalid config: {message}"),
            Self::NotFound(message) => write!(formatter, "not found: {message}"),
            Self::Storage(message) => write!(formatter, "storage error: {message}"),
        }
    }
}

impl Error for AppError {}

impl From<ConfigError> for AppError {
    fn from(error: ConfigError) -> Self {
        Self::InvalidConfig(error.to_string())
    }
}
