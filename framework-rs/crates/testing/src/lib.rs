#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use forklaunch_core::ServiceInfo;

mod environment;
pub use environment::{
    DatabaseType, TestContainerEndpoint, TestEnvConfig, TestEnvironmentError,
    setup_test_environment,
};

/// Deliberately non-authenticating fixtures, matching TypeScript `TEST_TOKENS`.
pub struct TestTokens {
    pub auth: &'static str,
    pub hmac: &'static str,
    pub hmac_invalid: &'static str,
}

pub const TEST_TOKENS: TestTokens = TestTokens {
    auth: "Bearer test-token",
    hmac: "HMAC keyId=test-key ts=1234567890 nonce=test-nonce signature=test-signature",
    hmac_invalid: "HMAC keyId=invalid-key ts=1234567890 nonce=invalid-nonce signature=invalid-signature",
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestService {
    pub info: ServiceInfo,
    pub env: BTreeMap<String, String>,
}

impl TestService {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            info: ServiceInfo::new(name, "test"),
            env: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.info.version = version.into();
        self
    }

    #[must_use]
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Add TypeScript-compatible test defaults and container endpoints, preserving
    /// existing unrelated variables. Custom configuration overrides the defaults.
    /// Pass `&service.env` to a child command's `envs` method.
    ///
    /// # Errors
    /// Returns an error when a configured container is missing a required port.
    pub fn with_test_environment(
        mut self,
        config: &TestEnvConfig,
    ) -> Result<Self, TestEnvironmentError> {
        self.env = setup_test_environment(config, self.env)?;
        Ok(self)
    }

    #[must_use]
    pub fn env(&self, key: &str) -> Option<&str> {
        self.env.get(key).map(String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotAssertion {
    pub name: String,
    pub actual: String,
}

impl SnapshotAssertion {
    #[must_use]
    pub fn new(name: impl Into<String>, actual: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            actual: actual.into(),
        }
    }

    #[must_use]
    pub fn matches(&self, expected: &str) -> bool {
        normalize_snapshot(&self.actual) == normalize_snapshot(expected)
    }
}

#[must_use]
pub fn normalize_snapshot(input: &str) -> String {
    input
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[must_use]
pub fn assert_contains_json_field(encoded_json: &str, field: &str) -> bool {
    encoded_json.contains(&format!("\"{field}\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_tracks_metadata_and_env() {
        let service = TestService::new("accounts")
            .with_version("1.2.3")
            .with_env("DATABASE_URL", "sqlite::memory:");

        assert_eq!(service.info.name, "accounts");
        assert_eq!(service.info.version, "1.2.3");
        assert_eq!(service.env("DATABASE_URL"), Some("sqlite::memory:"));
    }

    #[test]
    fn snapshot_assertions_ignore_blank_lines_and_trailing_whitespace() {
        let assertion = SnapshotAssertion::new("contract", "{\n  \"ok\": true\n}\n");

        assert!(assertion.matches("{\n  \"ok\": true   \n}\n\n"));
    }
}
