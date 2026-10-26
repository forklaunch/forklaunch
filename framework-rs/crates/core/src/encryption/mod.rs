//! Authenticated tenant/resource/purpose-bound encryption with explicit wire namespaces.
#[derive(Debug)]
pub enum EnvelopeError {
    InvalidInput,
    Encryption,
    /// A required `ENCRYPTION_*` setting is missing or not text; names the
    /// variable, never its value.
    Configuration(crate::config::ConfigError),
}
impl std::fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Configuration(error) => error.fmt(f),
            Self::InvalidInput | Self::Encryption => f.write_str("encrypted envelope unavailable"),
        }
    }
}
impl std::error::Error for EnvelopeError {}
/// Explicit wire namespace; changing it intentionally makes old ciphertext unreadable.
#[derive(Clone)]
pub struct EnvelopeProfile {
    namespace: String,
}
impl EnvelopeProfile {
    /// Create a bounded, unambiguous wire namespace.
    ///
    /// # Errors
    /// Rejects empty, oversized or non-ASCII namespaces and punctuation other than hyphen/underscore.
    pub fn new(namespace: &str) -> Result<Self, EnvelopeError> {
        if namespace.is_empty()
            || namespace.len() > 64
            || !namespace
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            return Err(EnvelopeError::InvalidInput);
        }
        Ok(Self {
            namespace: namespace.into(),
        })
    }
}
mod envelope;
mod environment;
pub use envelope::EnvelopeKey;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn namespace_is_explicit_authenticated_context() {
        let one = EnvelopeKey::new(EnvelopeProfile::new("app-one").unwrap(), [7; 32]);
        let two = EnvelopeKey::new(EnvelopeProfile::new("app-two").unwrap(), [7; 32]);
        let tenant = uuid::Uuid::new_v4();
        let item = uuid::Uuid::new_v4();
        let value = one.seal(tenant, item, "body", b"secret").unwrap();
        assert!(two.open(tenant, item, "body", &value).is_err());
        assert!(EnvelopeProfile::new("ambiguous:namespace").is_err());
    }
}
