//! Startup configuration read from the process environment.
//!
//! Configuration values have no in-code defaults. A value the process needs is
//! read with [`required`](crate::config::required), which fails startup with an error naming the missing
//! or malformed variable; the value itself comes from the deployment or the
//! generated `.env` files and Compose environment. [`optional`](crate::config::optional) is reserved for
//! variables whose absence is a documented choice (for example, an issuer
//! constraint that is not enforced when unset), never for supplying a fallback.
//!
//! Both readers are generic over the target type, so one call site reads and
//! parses:
//!
//! ```
//! use forklaunch_core::config;
//!
//! fn listener() -> Result<(String, u16), config::ConfigError> {
//!     let host: String = config::required("HOST")?;
//!     let port: u16 = config::required("PORT")?;
//!     Ok((host, port))
//! }
//!
//! let error = config::required::<u16>("EXAMPLE_UNSET_PORT").unwrap_err();
//! assert_eq!(
//!     error.to_string(),
//!     "environment variable EXAMPLE_UNSET_PORT is not set; it is required at startup"
//! );
//! ```
//!
//! An empty value counts as unset. Errors name the variable and the expected
//! type and never include the value, which may be a credential.
use std::{env, error::Error, fmt, str::FromStr};

/// Reads and parses a required environment variable.
///
/// # Errors
/// Returns a [`ConfigError`] when the variable is unset, empty, not UTF-8, or
/// does not parse as `T`.
pub fn required<T: FromStr>(name: &str) -> Result<T, ConfigError> {
    present(name, optional(name)?)
}

pub(crate) fn present<T>(name: &str, value: Option<T>) -> Result<T, ConfigError> {
    value.ok_or_else(|| ConfigError::new(name, Problem::Missing))
}

/// Reads and parses an environment variable whose absence is a documented
/// choice. Unset and empty values return `None`; a present value must parse.
///
/// # Errors
/// Returns a [`ConfigError`] when the variable is set but is not UTF-8 or does
/// not parse as `T`.
pub fn optional<T: FromStr>(name: &str) -> Result<Option<T>, ConfigError> {
    parse(name, env::var(name))
}

/// [`optional`] over an already-read value, for readers with injectable sources.
pub(crate) fn parse<T: FromStr>(
    name: &str,
    read: Result<String, env::VarError>,
) -> Result<Option<T>, ConfigError> {
    let value = match read {
        Ok(value) if value.is_empty() => return Ok(None),
        Ok(value) => value,
        Err(env::VarError::NotPresent) => return Ok(None),
        Err(env::VarError::NotUnicode(_)) => {
            return Err(ConfigError::new(name, Problem::NotUnicode));
        }
    };
    value.parse().map(Some).map_err(|_| {
        ConfigError::new(
            name,
            Problem::Invalid {
                expected: short_type_name(std::any::type_name::<T>()),
            },
        )
    })
}

/// A startup configuration variable that is missing or malformed. Its `Debug`
/// form is the message, so `main` returning this error reports it readably.
#[derive(Clone, PartialEq, Eq)]
pub struct ConfigError {
    name: String,
    problem: Problem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Problem {
    Missing,
    NotUnicode,
    Invalid { expected: String },
}

impl ConfigError {
    fn new(name: &str, problem: Problem) -> Self {
        Self {
            name: name.to_owned(),
            problem,
        }
    }

    /// The environment variable this error is about.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// True when the variable is unset or empty, as opposed to malformed.
    #[must_use]
    pub fn is_missing(&self) -> bool {
        self.problem == Problem::Missing
    }

    fn code(&self) -> &'static str {
        if self.is_missing() {
            "config_missing"
        } else {
            "config_invalid"
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = &self.name;
        match &self.problem {
            Problem::Missing => write!(
                formatter,
                "environment variable {name} is not set; it is required at startup"
            ),
            Problem::NotUnicode => {
                write!(formatter, "environment variable {name} is not valid UTF-8")
            }
            Problem::Invalid { expected } => {
                write!(
                    formatter,
                    "environment variable {name} is not a valid {expected}"
                )
            }
        }
    }
}

impl fmt::Debug for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ConfigError")
            .field(&self.to_string())
            .finish()
    }
}

impl Error for ConfigError {}

impl From<ConfigError> for crate::common::FrameworkError {
    fn from(error: ConfigError) -> Self {
        Self::new(error.code(), error.to_string())
    }
}

#[cfg(feature = "domain")]
impl From<ConfigError> for crate::domain::BlueprintError {
    fn from(error: ConfigError) -> Self {
        Self::new(error.code(), error.to_string())
    }
}

impl From<ConfigError> for std::io::Error {
    fn from(error: ConfigError) -> Self {
        Self::new(std::io::ErrorKind::InvalidInput, error)
    }
}

/// `core::net::socket_addr::SocketAddr` -> `SocketAddr`, keeping generic shape.
fn short_type_name(full: &str) -> String {
    let mut short = String::with_capacity(full.len());
    let mut path = String::new();
    for character in full.chars() {
        if character.is_alphanumeric() || matches!(character, '_' | ':') {
            path.push(character);
        } else {
            short.push_str(path.rsplit("::").next().unwrap_or_default());
            path.clear();
            short.push(character);
        }
    }
    short.push_str(path.rsplit("::").next().unwrap_or_default());
    short
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::OsString, net::SocketAddr};

    fn read<T: FromStr>(name: &str, value: Option<&str>) -> Result<T, ConfigError> {
        let value = value.map(str::to_owned).ok_or(env::VarError::NotPresent);
        present(name, parse(name, value)?)
    }

    #[test]
    fn required_reads_and_parses_the_variable() {
        assert_eq!(read::<u16>("PORT", Some("8080")), Ok(8080));
        assert_eq!(read::<String>("HOST", Some("0.0.0.0")).unwrap(), "0.0.0.0");
    }

    #[test]
    fn missing_and_empty_variables_name_the_variable() {
        for value in [None, Some("")] {
            let error = read::<String>("PORT", value).unwrap_err();
            assert!(error.is_missing());
            assert_eq!(error.name(), "PORT");
            assert_eq!(
                error.to_string(),
                "environment variable PORT is not set; it is required at startup"
            );
            let absent = value.map(str::to_owned).ok_or(env::VarError::NotPresent);
            assert_eq!(parse::<String>("PORT", absent), Ok(None));
        }
        let error = required::<u16>("FORKLAUNCH_CONFIG_TEST_UNSET").unwrap_err();
        assert!(error.is_missing());
        assert_eq!(optional::<u16>("FORKLAUNCH_CONFIG_TEST_UNSET"), Ok(None));
    }

    #[test]
    fn malformed_values_fail_without_echoing_the_value() {
        let port = read::<u16>("PORT", Some("secret-looking-value")).unwrap_err();
        assert!(!port.is_missing());
        assert_eq!(
            port.to_string(),
            "environment variable PORT is not a valid u16"
        );
        let address =
            read::<SocketAddr>("WORKER_HTTP_ADDR", Some("secret-looking-value")).unwrap_err();
        assert_eq!(
            address.to_string(),
            "environment variable WORKER_HTTP_ADDR is not a valid SocketAddr"
        );
        assert!(!format!("{address:?}").contains("secret"));
        assert_eq!(
            format!("{port:?}"),
            "ConfigError(\"environment variable PORT is not a valid u16\")"
        );
        let binary = parse::<String>("TOKEN", Err(env::VarError::NotUnicode(OsString::new())));
        assert_eq!(
            binary.unwrap_err().to_string(),
            "environment variable TOKEN is not valid UTF-8"
        );
    }

    #[test]
    fn optional_parses_present_values() {
        assert_eq!(parse::<u64>("LIMIT", Ok("25".into())), Ok(Some(25)));
    }

    #[test]
    fn errors_convert_to_framework_and_io_errors() {
        let error = read::<String>("PORT", None).unwrap_err();
        let framework = crate::common::FrameworkError::from(error.clone());
        assert_eq!(framework.code(), "config_missing");
        assert_eq!(framework.message(), error.to_string());
        let invalid = read::<u16>("PORT", Some("x")).unwrap_err();
        assert_eq!(
            crate::common::FrameworkError::from(invalid).code(),
            "config_invalid"
        );
        let io = std::io::Error::from(error);
        assert_eq!(io.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn type_names_drop_module_paths() {
        assert_eq!(
            short_type_name("core::net::socket_addr::SocketAddr"),
            "SocketAddr"
        );
        assert_eq!(
            short_type_name("alloc::vec::Vec<alloc::string::String>"),
            "Vec<String>"
        );
        assert_eq!(short_type_name("u16"), "u16");
    }
}
