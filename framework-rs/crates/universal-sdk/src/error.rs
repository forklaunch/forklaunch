use crate::sse;
use std::{error::Error, fmt};
#[derive(Debug)]
pub enum SdkError {
    Transport(reqwest::Error),
    TransportPolicy,
    Encode(serde_json::Error),
    ResponseHeaders,
    Contract(&'static str),
    Multipart(&'static str),
    Sse(sse::SseError),
    Decode(serde_json::Error),
    Stream(&'static str),
}

impl fmt::Display for SdkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(error) => write!(formatter, "{error}"),
            Self::TransportPolicy => {
                formatter.write_str("HTTP TLS is required outside loopback development")
            }
            Self::Encode(error) | Self::Decode(error) => write!(formatter, "{error}"),
            Self::Contract(message) | Self::Multipart(message) | Self::Stream(message) => {
                formatter.write_str(message)
            }
            Self::ResponseHeaders => formatter.write_str("invalid response headers"),
            Self::Sse(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for SdkError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transport(error) => Some(error),
            Self::Encode(error) | Self::Decode(error) => Some(error),
            Self::Sse(error) => Some(error),
            Self::TransportPolicy
            | Self::ResponseHeaders
            | Self::Contract(_)
            | Self::Multipart(_)
            | Self::Stream(_) => None,
        }
    }
}

impl From<reqwest::Error> for SdkError {
    fn from(error: reqwest::Error) -> Self {
        Self::Transport(error.without_url())
    }
}

impl From<sse::SseError> for SdkError {
    fn from(error: sse::SseError) -> Self {
        Self::Sse(error)
    }
}
impl From<serde_json::Error> for SdkError {
    fn from(error: serde_json::Error) -> Self {
        Self::Decode(error)
    }
}
