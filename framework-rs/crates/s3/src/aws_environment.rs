//! S3 environment configuration and the official AWS default credential chain.
use std::time::Duration;

use super::{AwsS3ObjectStore, AwsS3Options, aws_sdk_s3};
use crate::S3ObjectStoreConfig;
use aws_sdk_s3::config::{Credentials, Region};
use forklaunch_core::common::{FrameworkError, FrameworkResult};

/// Resolved settings; credentials deliberately have no application Debug output.
pub struct S3Environment {
    pub store: S3ObjectStoreConfig,
    pub options: AwsS3Options,
    endpoint: Option<String>,
    credentials: Option<Credentials>,
}

impl S3Environment {
    /// # Errors
    /// Rejects a missing or empty bucket. Construction does not load credentials or connect.
    pub fn from_env() -> FrameworkResult<Self> {
        Self::from_vars(|name| std::env::var(name).ok())
    }

    /// Resolve the same settings from an application-owned configuration source.
    ///
    /// # Errors
    /// Rejects a missing or empty bucket.
    pub fn from_vars(mut get: impl FnMut(&str) -> Option<String>) -> FrameworkResult<Self> {
        let bucket = get("S3_BUCKET")
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| FrameworkError::new("s3_config_invalid", "S3_BUCKET is required"))?;
        let region = get("S3_REGION").filter(|value| !value.is_empty());
        let endpoint = get("S3_URL").filter(|value| !value.is_empty());
        let access = get("S3_ACCESS_KEY_ID").filter(|value| !value.is_empty());
        let secret = get("S3_SECRET_ACCESS_KEY").filter(|value| !value.is_empty());
        // A partial pair falls back to the default chain, as s3ClientConfig does.
        let credentials = access.zip(secret).map(|(access, secret)| {
            Credentials::new(access, secret, None, None, "forklaunch-environment")
        });
        let prefix = get("S3_PREFIX")
            .unwrap_or_default()
            .trim_matches('/')
            .to_owned();
        let defaults = AwsS3Options::default();
        let options = AwsS3Options {
            prefix: if prefix.is_empty() {
                prefix
            } else {
                format!("{prefix}/")
            },
            max_upload_lifetime: seconds(
                get("S3_PRESIGN_MAX_UPLOAD_SECONDS"),
                defaults.max_upload_lifetime,
            ),
            max_download_lifetime: seconds(
                get("S3_PRESIGN_MAX_DOWNLOAD_SECONDS"),
                defaults.max_download_lifetime,
            ),
            ..defaults
        };
        Ok(Self {
            store: S3ObjectStoreConfig { bucket, region },
            options,
            endpoint,
            credentials,
        })
    }

    #[must_use]
    pub fn endpoint(&self) -> Option<&str> {
        self.endpoint.as_deref()
    }

    #[must_use]
    pub const fn static_credentials(&self) -> bool {
        self.credentials.is_some()
    }

    /// Load the official SDK region/credential chain, then construct an injected client.
    /// Construction never creates a bucket.
    ///
    /// # Errors
    /// Rejects invalid adapter options or namespaces.
    pub async fn into_store(self) -> FrameworkResult<AwsS3ObjectStore> {
        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
        if let Some(region) = &self.store.region {
            loader = loader.region(Region::new(region.clone()));
        }
        if let Some(credentials) = self.credentials {
            loader = loader.credentials_provider(credentials);
        }
        let sdk = loader.load().await;
        let mut config = aws_sdk_s3::config::Builder::from(&sdk);
        if let Some(endpoint) = self.endpoint {
            config = config.endpoint_url(endpoint).force_path_style(true);
        }
        AwsS3ObjectStore::new(
            aws_sdk_s3::Client::from_conf(config.build()),
            self.store,
            self.options,
        )
    }
}

// JavaScript Number rounds integer radix inputs to f64 too.
#[allow(clippy::cast_precision_loss)]
fn seconds(value: Option<String>, fallback: Duration) -> Duration {
    let Some(value) = value else {
        return fallback;
    };
    let value = value.trim_matches(|character: char| {
        matches!(
            character,
            '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
                ..='\u{200a}'
                    | '\u{2028}'
                    | '\u{2029}'
                    | '\u{202f}'
                    | '\u{205f}'
                    | '\u{3000}'
                    | '\u{feff}'
        )
    });
    let number = if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).ok().map(|value| value as f64)
    } else if let Some(octal) = value
        .strip_prefix("0o")
        .or_else(|| value.strip_prefix("0O"))
    {
        u64::from_str_radix(octal, 8).ok().map(|value| value as f64)
    } else if let Some(binary) = value
        .strip_prefix("0b")
        .or_else(|| value.strip_prefix("0B"))
    {
        u64::from_str_radix(binary, 2)
            .ok()
            .map(|value| value as f64)
    } else {
        value.parse::<f64>().ok()
    };
    number
        .filter(|number| number.is_finite() && *number > 0.0)
        .and_then(|number| Duration::try_from_secs_f64(number.floor()).ok())
        .unwrap_or(fallback)
}
