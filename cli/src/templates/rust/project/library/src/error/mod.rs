use std::{fmt, error::Error, result};

pub type Result<T> = result::Result<T, LibraryError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryError {
    MissingItem(String),
}

impl fmt::Display for LibraryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingItem(key) => write!(formatter, "missing library item: {key}"),
        }
    }
}

impl Error for LibraryError {}
