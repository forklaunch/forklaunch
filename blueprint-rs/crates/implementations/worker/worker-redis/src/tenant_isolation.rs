//! Platform namespace conformance. Redis ACLs enforce the actual tenant boundary.
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RedisKeyScope {
    prefix: Option<String>,
}

impl RedisKeyScope {
    /// Resolves the same platform variables and precedence as TypeScript `BullMQ` workers.
    /// # Errors
    /// Rejects prefixes containing control characters/hash tags or exceeding 256 bytes.
    pub fn from_env() -> BlueprintResult<Self> {
        Self::from_vars(|name| std::env::var(name).ok())
    }

    /// # Errors
    /// Rejects invalid prefixes. A missing prefix preserves dedicated-cache legacy keys.
    pub fn from_vars(mut lookup: impl FnMut(&str) -> Option<String>) -> BlueprintResult<Self> {
        for name in ["REDIS_KEY_PREFIX", "FORKLAUNCH_REDIS_KEY_PREFIX"] {
            if let Some(value) = lookup(name) {
                let value = value.trim_matches(js_whitespace);
                if !value.is_empty() {
                    return Self::with_prefix(value);
                }
            }
        }
        Ok(Self::default())
    }

    /// Constructs an explicit trusted scope, suitable for dependency-injected configuration.
    /// # Errors
    /// Rejects invalid prefixes; trailing ':' separators are normalized.
    pub fn with_prefix(prefix: &str) -> BlueprintResult<Self> {
        let prefix = prefix.trim_matches(js_whitespace).trim_end_matches(':');
        if prefix.len() > 256
            || prefix
                .chars()
                .any(|character| character.is_control() || matches!(character, '{' | '}'))
        {
            return Err(BlueprintError::new(
                "worker_invalid_prefix",
                "Invalid Redis worker key prefix",
            ));
        }
        Ok(Self {
            prefix: (!prefix.is_empty()).then(|| prefix.to_owned()),
        })
    }

    #[must_use]
    pub fn prefix(&self) -> Option<&str> {
        self.prefix.as_deref()
    }

    #[must_use]
    pub fn qualify(&self, local: &str) -> String {
        self.prefix
            .as_ref()
            .map_or_else(|| local.to_owned(), |prefix| format!("{prefix}:{local}"))
    }
}

fn js_whitespace(character: char) -> bool {
    matches!(character,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}

pub(crate) fn isolation_error(error: &redis::RedisError) -> Option<BlueprintError> {
    error.code().filter(|code|code.eq_ignore_ascii_case("NOPERM")).map(|_|BlueprintError::new("worker_tenant_isolation_error","Redis ACL denied worker operation; verify the application key/channel prefix and permissions"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_prefix_precedence_normalization_and_dedicated_compatibility() {
        let scope = RedisKeyScope::from_vars(|name| match name {
            "REDIS_KEY_PREFIX" => Some("\u{feff}app::: \u{2028}".into()),
            _ => Some("alias:".into()),
        })
        .unwrap();
        assert_eq!(scope.prefix(), Some("app"));
        assert_eq!(
            scope.qualify("fl_worker:{slot}:jobs"),
            "app:fl_worker:{slot}:jobs"
        );
        let alias = RedisKeyScope::from_vars(|name| match name {
            "REDIS_KEY_PREFIX" => Some(" ".into()),
            _ => Some("alias:".into()),
        })
        .unwrap();
        assert_eq!(alias.prefix(), Some("alias"));
        let none = RedisKeyScope::from_vars(|_| None).unwrap();
        assert_eq!(none.qualify("legacy-key"), "legacy-key");
        assert!(
            RedisKeyScope::with_prefix(":::")
                .unwrap()
                .prefix()
                .is_none()
        );
        for bad in ["bad\ninside", "prefix{other-slot}", &"x".repeat(257)] {
            assert!(RedisKeyScope::with_prefix(bad).is_err());
        }
        let denied =
            redis::parse_redis_value(b"-NOPERM credential-and-private-key-must-not-escape\r\n")
                .unwrap()
                .extract_error()
                .unwrap_err();
        let error = isolation_error(&denied).unwrap();
        assert_eq!(error.code, "worker_tenant_isolation_error");
        assert!(!error.to_string().contains("credential-and-private"));
        assert!(
            isolation_error(&redis::RedisError::from((
                redis::ErrorKind::Io,
                "ordinary failure"
            )))
            .is_none()
        );
    }
}
