use crate::error::{AppError, Result};
use forklaunch_core::config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceConfig {
    pub name: String,
    pub protocol: String,
    pub host: String,
    pub port: u16,
    pub public_origin: Option<String>,
    /// Exact browser origins allowed to call cross-origin (`CORS_ORIGINS`,
    /// comma-separated). Others, and `*`, are refused.
    pub cors_origins: Vec<String>,
    pub version: String,
    pub docs_path: String,
    pub docs_server_urls: Vec<String>,
    pub docs_server_descriptions: Vec<String>,
}

impl ServiceConfig {
    /// Reads the service settings. Each is required at startup; a missing or
    /// malformed variable is an error naming it. `PUBLIC_ORIGIN`, `CORS_ORIGINS` and
    /// `DOCS_SERVER_URLS`/`DOCS_SERVER_DESCRIPTIONS` are optional: without them
    /// the service is reached at, and documents, its own listener address.
    pub fn from_env() -> Result<Self> {
        let protocol: String = config::required("PROTOCOL")?;
        let host: String = config::required("HOST")?;
        let port: u16 = config::required("PORT")?;
        let docs_server_urls = config::optional::<String>("DOCS_SERVER_URLS")?
            .map(|value| csv_values(&value))
            .filter(|values| !values.is_empty())
            .unwrap_or_else(|| vec![format!("{protocol}://{host}:{port}")]);
        let docs_server_descriptions = config::optional::<String>("DOCS_SERVER_DESCRIPTIONS")?
            .map(|value| csv_values(&value))
            .unwrap_or_default();

        Ok(Self {
            name: "{{{name}}}".to_string(),
            protocol,
            host,
            port,
            public_origin: config::optional("PUBLIC_ORIGIN")?,
            cors_origins: config::optional::<String>("CORS_ORIGINS")?
                .map(|value| csv_values(&value))
                .unwrap_or_default(),
            version: config::required("VERSION")?,
            docs_path: config::required("DOCS_PATH")?,
            docs_server_urls,
            docs_server_descriptions,
        })
    }

    #[must_use]
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    #[must_use]
    pub fn origin(&self) -> String {
        self.public_origin.clone().unwrap_or_else(|| {
            format!("{}://{}:{}", self.protocol, self.host, self.port)
        })
    }

    /// Resolve an ephemeral listener port without replacing configured public URLs.
    pub fn set_bound_port(&mut self, port: u16) {
        if self.port == 0 {
            let previous = format!("{}://{}:0", self.protocol, self.host);
            self.port = port;
            for server in &mut self.docs_server_urls {
                if *server == previous {
                    *server = format!("{}://{}:{}", self.protocol, self.host, self.port);
                }
            }
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(AppError::InvalidConfig("service name cannot be empty".to_string()));
        }
        if self.host.trim().is_empty() {
            return Err(AppError::InvalidConfig("service host cannot be empty".to_string()));
        }
        if self.protocol.trim().is_empty() {
            return Err(AppError::InvalidConfig(
                "service protocol cannot be empty".to_string(),
            ));
        }
        if !self.docs_path.starts_with('/') {
            return Err(AppError::InvalidConfig(
                "docs path must start with '/'".to_string(),
            ));
        }
        if self.docs_server_urls.iter().any(|url| url.trim().is_empty()) {
            return Err(AppError::InvalidConfig(
                "docs server urls cannot contain empty values".to_string(),
            ));
        }
        Ok(())
    }
}

/// A fixed configuration for tests; processes use [`ServiceConfig::from_env`].
impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            name: "{{{name}}}".to_string(),
            protocol: "http".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8080,
            public_origin: None,
            cors_origins: Vec::new(),
            version: "v1".to_string(),
            docs_path: "/docs".to_string(),
            docs_server_urls: vec!["http://127.0.0.1:8080".to_string()],
            docs_server_descriptions: vec!["Main Server".to_string()],
        }
    }
}

fn csv_values(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}
