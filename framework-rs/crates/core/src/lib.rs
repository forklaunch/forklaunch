#![forbid(unsafe_code)]
extern crate self as forklaunch_core;

/// Agents: MCP-shaped tools, approvals and the model run loop.
#[cfg(feature = "agent-contract")]
pub mod agent;
#[cfg(feature = "auth")]
pub mod auth;
#[cfg(feature = "domain")]
pub mod domain;
#[cfg(feature = "http")]
pub mod http;

#[cfg(feature = "encryption")]
pub mod encryption;

#[cfg(feature = "openapi")]
pub mod openapi;

#[cfg(feature = "mappers")]
pub mod mappers;

/// Required and optional startup settings read from the environment.
pub mod config;
pub mod persistence;
pub mod telemetry;

pub use forklaunch_common as common;
pub mod contract;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEnvironment {
    Local,
    Test,
    Staging,
    Production,
}

impl RuntimeEnvironment {
    #[must_use]
    pub fn from_env_name(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "production" | "prod" => Self::Production,
            "staging" | "stage" => Self::Staging,
            "test" => Self::Test,
            _ => Self::Local,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Test => "test",
            Self::Staging => "staging",
            Self::Production => "production",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceInfo {
    pub name: String,
    pub version: String,
}

impl ServiceInfo {
    #[must_use]
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
        }
    }

    #[must_use]
    pub fn contract(self) -> contract::ServiceContract {
        contract::ServiceContract::new(self.name, self.version)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub environment: RuntimeEnvironment,
    pub service: ServiceInfo,
    pub port: u16,
}

impl RuntimeConfig {
    #[must_use]
    pub fn new(service: ServiceInfo) -> Self {
        Self {
            environment: RuntimeEnvironment::Local,
            service,
            port: 3000,
        }
    }

    #[must_use]
    pub const fn with_environment(mut self, environment: RuntimeEnvironment) -> Self {
        self.environment = environment;
        self
    }

    #[must_use]
    pub const fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// # Errors
    ///
    /// Returns an error when service metadata or network settings are invalid.
    pub fn validate(&self) -> common::FrameworkResult<()> {
        if self.service.name.trim().is_empty() {
            return Err(common::FrameworkError::new(
                "runtime_config_invalid",
                "service name is required",
            ));
        }

        if self.port == 0 {
            return Err(common::FrameworkError::new(
                "runtime_config_invalid",
                "port must be greater than zero",
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_config_validates_service_metadata() {
        let config = RuntimeConfig::new(ServiceInfo::new("accounts", "0.1.0"))
            .with_environment(RuntimeEnvironment::Test)
            .with_port(8080);

        assert!(config.validate().is_ok());
        assert_eq!(config.environment.as_str(), "test");
    }
}

/// MCP contract projection and authenticated transport.
#[cfg(feature = "mcp")]
pub mod mcp;

/// WebSocket contract descriptions and shared bounded message codecs.
#[cfg(feature = "ws")]
pub mod ws;

#[cfg(feature = "asyncapi")]
pub mod asyncapi;
#[cfg(feature = "build")]
pub mod build;
#[cfg(feature = "codegen")]
pub mod codegen;
#[cfg(feature = "rust-sdk")]
pub mod rust_sdk;
#[cfg(feature = "typescript")]
pub mod typescript;
