/// Only fixed classifications cross the queue result/error boundary. Never wrap
/// transport responses, parser diagnostics, keys or decrypted job contents here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessorError {
    InvalidEnvelope,
    Configuration,
    Failed,
    Timeout,
}

impl ProcessorError {
    #[must_use]
    pub fn retryable(self) -> bool {
        matches!(self, Self::Failed | Self::Timeout)
    }
}

impl std::fmt::Display for ProcessorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidEnvelope => "worker payload rejected",
            Self::Configuration => "worker configuration unavailable",
            Self::Failed => "worker processing failed",
            Self::Timeout => "worker processing timed out",
        })
    }
}

impl std::error::Error for ProcessorError {}
