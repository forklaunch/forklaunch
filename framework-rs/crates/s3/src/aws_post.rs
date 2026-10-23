//! Fixed-key browser upload grants using S3 Signature Version 4 POST policies.
use super::{AwsS3ObjectStore, error};
use aws_sdk_s3::{config::Credentials, presigning::PresigningConfig};
use aws_smithy_runtime_api::client::auth::AuthScheme as _;
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{DateTime, SecondsFormat, Utc};
use forklaunch_core::common::FrameworkResult;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::json;
use sha2::Sha256;
use std::{
    collections::BTreeMap,
    fmt::{self, Write as _},
    time::{Duration, SystemTime},
};
use zeroize::Zeroizing;

/// A positive byte bound and exact content type enforced by S3 itself.
#[derive(Debug, Clone)]
pub struct PresignUploadOptions {
    pub content_type: String,
    pub max_bytes: u64,
    pub expires_in: Option<Duration>,
}

/// Multipart form fields for one namespaced object. Treat the fields as a bearer
/// grant, submit them unmodified, and append the file as the final form field.
#[derive(Clone, PartialEq, Eq)]
pub struct PresignedUpload {
    pub url: String,
    pub fields: BTreeMap<String, String>,
    pub key: String,
    pub expires_at: SystemTime,
}
impl fmt::Debug for PresignedUpload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PresignedUpload")
            .field("key", &self.key)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

impl AwsS3ObjectStore {
    /// Signs an exact object key, exact content type and a `1..=max_bytes` policy.
    /// Credential and endpoint resolution use the injected AWS client and the
    /// configured operation deadline. No S3 request or bucket creation occurs.
    /// Temporary credentials shorten the returned expiration when necessary.
    ///
    /// # Errors
    /// Rejects invalid keys/types/byte bounds/lifetimes, unavailable or expired
    /// credentials and unsupported signing scopes (including SigV4a/Express).
    pub async fn presign_upload(
        &self,
        key: &str,
        options: &PresignUploadOptions,
    ) -> FrameworkResult<PresignedUpload> {
        self.bounded(self.presign_upload_inner(key, options)).await
    }

    async fn presign_upload_inner(
        &self,
        key: &str,
        options: &PresignUploadOptions,
    ) -> FrameworkResult<PresignedUpload> {
        if self.config.bucket.contains(':') || self.config.bucket.ends_with("--x-s3") {
            return Err(error(
                "s3_presign_invalid",
                "POST grants require an ordinary bucket endpoint",
            ));
        }
        let (_, wire) = self.wire_key(key)?;
        let lifetime = options
            .expires_in
            .unwrap_or(self.options.max_upload_lifetime);
        if lifetime.as_secs() == 0
            || lifetime > self.options.max_upload_lifetime
            || options.max_bytes == 0
            || options.max_bytes > self.options.max_object_bytes as u64
            || options.content_type.is_empty()
            || options.content_type.len() > 1024
            || options.content_type.chars().any(char::is_control)
            || wire.contains("${filename}")
        {
            return Err(error(
                "s3_presign_invalid",
                "upload requires a fixed key, content type and positive byte/lifetime bounds within configured caps",
            ));
        }
        let lifetime = Duration::from_secs(lifetime.as_secs());
        let now = SystemTime::now();
        let (mut url, credentials) = self.resolve_post_signer(now, lifetime).await?;
        let mut expires_at = now.checked_add(lifetime).ok_or_else(signing_error)?;
        if let Some(expiration) = credentials.expiry() {
            expires_at = expires_at.min(expiration);
        }
        if expires_at <= now {
            return Err(signing_error());
        }
        let query: BTreeMap<_, _> = url.query_pairs().into_owned().collect();
        let scope = query.get("X-Amz-Credential").ok_or_else(signing_error)?;
        let date = query.get("X-Amz-Date").ok_or_else(signing_error)?;
        let parts: Vec<_> = scope.split('/').collect();
        if query.get("X-Amz-Algorithm").map(String::as_str) != Some("AWS4-HMAC-SHA256")
            || parts.len() != 5
            || parts[0] != credentials.access_key_id()
            || parts[3] != "s3"
            || parts[4] != "aws4_request"
            || parts[1].len() != 8
            || parts[2].is_empty()
        {
            return Err(signing_error());
        }
        url.set_query(None);
        url.set_fragment(None);
        let mut fields: BTreeMap<String, String> = BTreeMap::from([
            ("bucket".into(), self.config.bucket.clone()),
            ("key".into(), wire),
            ("Content-Type".into(), options.content_type.clone()),
            ("x-amz-algorithm".into(), "AWS4-HMAC-SHA256".into()),
            ("x-amz-credential".into(), scope.clone()),
            ("x-amz-date".into(), date.clone()),
        ]);
        if let Some(token) = credentials.session_token() {
            fields.insert("x-amz-security-token".into(), token.to_owned());
        }
        let mut conditions: Vec<_> = fields
            .iter()
            .map(|(key, value)| json!({key:value}))
            .collect();
        conditions.push(json!(["content-length-range", 1, options.max_bytes]));
        let policy = STANDARD.encode(serde_json::to_vec(&json!({
                "expiration":DateTime::<Utc>::from(expires_at).to_rfc3339_opts(SecondsFormat::Millis,true),
                "conditions":conditions,
            })).map_err(|_| signing_error())?);
        let signature = sign_policy(credentials.secret_access_key(), parts[1], parts[2], &policy)?;
        fields.insert("policy".into(), policy);
        fields.insert("x-amz-signature".into(), signature);
        Ok(PresignedUpload {
            url: url.into(),
            fields,
            key: key.to_owned(),
            expires_at,
        })
    }
    async fn resolve_post_signer(
        &self,
        now: SystemTime,
        lifetime: Duration,
    ) -> FrameworkResult<(url::Url, Credentials)> {
        const SENTINEL: &str = "__forklaunch_post_endpoint__";
        // Let the official endpoint/signing resolver choose the bucket URL
        // and credential scope. Capture the very identity used for signing
        // so credential rotation cannot mix a scope/token with another secret. The sentinel key is
        // internal and never sent to S3.
        let signing = PresigningConfig::builder()
            .start_time(now)
            .expires_in(lifetime)
            .build()
            .map_err(|_| signing_error())?;
        let captured = std::sync::Arc::new(std::sync::Mutex::new(None));
        let inner = self
            .client
            .config()
            .auth_schemes()
            .find(|scheme| scheme.scheme_id().inner() == "aws.auth#sigv4")
            .unwrap_or_else(|| {
                aws_smithy_runtime_api::client::auth::SharedAuthScheme::new(
                    aws_runtime::auth::sigv4::SigV4AuthScheme::new(),
                )
            });
        let resolved = self
            .client
            .get_object()
            .bucket(&self.config.bucket)
            .key(SENTINEL)
            .customize()
            .config_override(aws_sdk_s3::config::Builder::new().push_auth_scheme(
                CaptureCredentials {
                    inner,
                    captured: std::sync::Arc::clone(&captured),
                },
            ))
            .presigned(signing)
            .await
            .map_err(|_| signing_error())?;
        let credentials = captured
            .lock()
            .map_err(|_| signing_error())?
            .take()
            .ok_or_else(signing_error)?;
        let mut url = url::Url::parse(resolved.uri()).map_err(|_| signing_error())?;
        let path = url
            .path()
            .strip_suffix(SENTINEL)
            .ok_or_else(signing_error)?
            .to_owned();
        url.set_path(&path);
        Ok((url, credentials))
    }
}

fn signing_error() -> forklaunch_core::common::FrameworkError {
    error(
        "s3_signing_failed",
        "unable to resolve supported S3 POST signing credentials and endpoint",
    )
}

fn sign_policy(secret: &str, date: &str, region: &str, policy: &str) -> FrameworkResult<String> {
    fn hmac(key: &[u8], value: &[u8]) -> FrameworkResult<Zeroizing<Vec<u8>>> {
        let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| signing_error())?;
        mac.update(value);
        Ok(Zeroizing::new(mac.finalize().into_bytes().to_vec()))
    }
    let initial = Zeroizing::new(format!("AWS4{secret}"));
    let date_key = hmac(initial.as_bytes(), date.as_bytes())?;
    let region_key = hmac(&date_key, region.as_bytes())?;
    let service_key = hmac(&region_key, b"s3")?;
    let signing_key = hmac(&service_key, b"aws4_request")?;
    let signature = hmac(&signing_key, policy.as_bytes())?;
    let mut encoded = String::with_capacity(64);
    for byte in signature.iter() {
        let _ = write!(encoded, "{byte:02x}");
    }
    Ok(encoded)
}

// Delegate the SDK's SigV4 auth scheme, capturing the exact resolved identity
// passed to its signer. Provider refresh/cache policy remains owned by the SDK.
#[derive(Debug)]
struct CaptureCredentials {
    inner: aws_smithy_runtime_api::client::auth::SharedAuthScheme,
    captured: std::sync::Arc<std::sync::Mutex<Option<Credentials>>>,
}
impl aws_smithy_runtime_api::client::auth::AuthScheme for CaptureCredentials {
    fn scheme_id(&self) -> aws_smithy_runtime_api::client::auth::AuthSchemeId {
        self.inner.scheme_id()
    }
    fn identity_resolver(
        &self,
        resolvers: &dyn aws_smithy_runtime_api::client::runtime_components::GetIdentityResolver,
    ) -> Option<aws_smithy_runtime_api::client::identity::SharedIdentityResolver> {
        self.inner.identity_resolver(resolvers)
    }
    fn signer(&self) -> &dyn aws_smithy_runtime_api::client::auth::Sign {
        self
    }
}
impl aws_smithy_runtime_api::client::auth::Sign for CaptureCredentials {
    fn sign_http_request(
        &self,
        request: &mut aws_smithy_runtime_api::client::orchestrator::HttpRequest,
        identity: &aws_smithy_runtime_api::client::identity::Identity,
        endpoint: aws_smithy_runtime_api::client::auth::AuthSchemeEndpointConfig<'_>,
        runtime: &aws_smithy_runtime_api::client::runtime_components::RuntimeComponents,
        config: &aws_smithy_types::config_bag::ConfigBag,
    ) -> Result<(), aws_smithy_runtime_api::box_error::BoxError> {
        if let Some(credentials) = identity.data::<Credentials>() {
            *self.captured.lock().map_err(|_| signing_error())? = Some(credentials.clone());
        }
        self.inner
            .signer()
            .sign_http_request(request, identity, endpoint, runtime, config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{S3ObjectStoreConfig, aws::AwsS3Options};
    use aws_credential_types::provider::{ProvideCredentials, future};
    use aws_sdk_s3::{
        Client,
        config::{Region, retry::RetryConfig},
    };
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    fn store(provider: impl ProvideCredentials + 'static, endpoint: &str) -> AwsS3ObjectStore {
        let client = Client::from_conf(
            aws_sdk_s3::Config::builder()
                .region(Region::new("us-east-1"))
                .credentials_provider(provider)
                .endpoint_url(endpoint)
                .force_path_style(true)
                .retry_config(RetryConfig::disabled())
                .behavior_version_latest()
                .build(),
        );
        AwsS3ObjectStore::new(
            client,
            S3ObjectStoreConfig::new("post-parity"),
            AwsS3Options {
                prefix: "tenant/owned".into(),
                operation_timeout: Duration::from_secs(2),
                ..AwsS3Options::default()
            },
        )
        .unwrap()
    }
    fn credentials() -> Credentials {
        Credentials::new(
            "post-test-access",
            "post-test-secret-only",
            None,
            None,
            "post-test",
        )
    }
    fn options() -> PresignUploadOptions {
        PresignUploadOptions {
            content_type: "text/plain".into(),
            max_bytes: 1024,
            expires_in: Some(Duration::from_secs(60)),
        }
    }
    fn policy(grant: &PresignedUpload) -> serde_json::Value {
        serde_json::from_slice(&STANDARD.decode(&grant.fields["policy"]).unwrap()).unwrap()
    }

    #[tokio::test]
    async fn post_grants_bind_every_field_and_exact_key_without_network_requests() {
        let store = store(credentials(), "http://127.0.0.1:9/gateway/");
        let before = SystemTime::now();
        let grant = store.presign_upload("/a/é file", &options()).await.unwrap();
        assert_eq!(grant.url, "http://127.0.0.1:9/gateway/post-parity/");
        assert_eq!(grant.key, "/a/é file");
        assert_eq!(grant.fields["key"], "tenant/owned/a/é file");
        assert_eq!(grant.fields["Content-Type"], "text/plain");
        assert!(grant.expires_at >= before + Duration::from_secs(60));
        assert!(grant.expires_at <= SystemTime::now() + Duration::from_secs(60));
        let policy = policy(&grant);
        let conditions = policy["conditions"].as_array().unwrap();
        for (key, value) in &grant.fields {
            if !matches!(key.as_str(), "policy" | "x-amz-signature") {
                assert!(conditions.contains(&json!({key:value})), "{key}");
            }
        }
        assert!(conditions.contains(&json!(["content-length-range", 1, 1024])));
        let scope: Vec<_> = grant.fields["x-amz-credential"].split('/').collect();
        assert_eq!(
            grant.fields["x-amz-signature"],
            sign_policy(
                "post-test-secret-only",
                scope[1],
                scope[2],
                &grant.fields["policy"]
            )
            .unwrap()
        );
        let debug = format!("{grant:?}");
        assert!(!debug.contains(&grant.fields["x-amz-signature"]));
        assert!(!debug.contains(&grant.fields["policy"]));
        assert_eq!(
            store
                .presign_upload(
                    "default",
                    &PresignUploadOptions {
                        expires_in: None,
                        ..options()
                    }
                )
                .await
                .unwrap()
                .expires_at
                .duration_since(SystemTime::now())
                .unwrap()
                .as_secs(),
            899
        );
    }

    #[derive(Debug)]
    struct CountingProvider {
        calls: Arc<AtomicUsize>,
        expiry: SystemTime,
    }
    impl ProvideCredentials for CountingProvider {
        fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
        where
            Self: 'a,
        {
            self.calls.fetch_add(1, Ordering::SeqCst);
            future::ProvideCredentials::ready(Ok(Credentials::new(
                "post-test-access",
                "post-test-secret-only",
                Some("temporary-test-token".into()),
                Some(self.expiry),
                "post-test",
            )))
        }
    }
    #[tokio::test]
    async fn temporary_credentials_are_bound_and_grant_expiry_never_exceeds_them() {
        let calls = Arc::new(AtomicUsize::new(0));
        let expiry = SystemTime::now() + Duration::from_secs(600);
        let store = store(
            CountingProvider {
                calls: Arc::clone(&calls),
                expiry,
            },
            "http://127.0.0.1:9",
        );
        for _ in 0..2 {
            let grant = store
                .presign_upload(
                    "temporary",
                    &PresignUploadOptions {
                        expires_in: None,
                        ..options()
                    },
                )
                .await
                .unwrap();
            assert_eq!(grant.expires_at, expiry);
            assert_eq!(grant.fields["x-amz-security-token"], "temporary-test-token");
            assert!(
                policy(&grant)["conditions"]
                    .as_array()
                    .unwrap()
                    .contains(&json!({"x-amz-security-token":"temporary-test-token"}))
            );
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "SDK cache must retain its provider behavior"
        );
    }

    #[tokio::test]
    async fn invalid_options_fail_before_credentials_are_resolved() {
        let calls = Arc::new(AtomicUsize::new(0));
        let store = store(
            CountingProvider {
                calls: Arc::clone(&calls),
                expiry: SystemTime::now() + Duration::from_secs(600),
            },
            "http://127.0.0.1:9",
        );
        let invalid = [
            PresignUploadOptions {
                max_bytes: 0,
                ..options()
            },
            PresignUploadOptions {
                max_bytes: 16 * 1024 * 1024 + 1,
                ..options()
            },
            PresignUploadOptions {
                content_type: String::new(),
                ..options()
            },
            PresignUploadOptions {
                content_type: "text/plain\r\ninjected".into(),
                ..options()
            },
            PresignUploadOptions {
                content_type: "a".repeat(1025),
                ..options()
            },
            PresignUploadOptions {
                expires_in: Some(Duration::ZERO),
                ..options()
            },
            PresignUploadOptions {
                expires_in: Some(Duration::from_millis(500)),
                ..options()
            },
            PresignUploadOptions {
                expires_in: Some(Duration::from_secs(901)),
                ..options()
            },
        ];
        for options in invalid {
            assert_eq!(
                store
                    .presign_upload("valid", &options)
                    .await
                    .unwrap_err()
                    .code(),
                "s3_presign_invalid"
            );
        }
        for key in ["/", "../escape", "file/${filename}"] {
            assert!(store.presign_upload(key, &options()).await.is_err());
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[derive(Debug)]
    struct BlockedProvider;
    impl ProvideCredentials for BlockedProvider {
        fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
        where
            Self: 'a,
        {
            future::ProvideCredentials::new(std::future::pending())
        }
    }
    #[tokio::test]
    async fn stalled_credentials_are_bounded_by_the_operation_deadline() {
        let mut store = store(BlockedProvider, "http://127.0.0.1:9");
        store.options.operation_timeout = Duration::from_millis(50);
        assert_eq!(
            store
                .presign_upload("valid", &options())
                .await
                .unwrap_err()
                .code(),
            "s3_timeout"
        );
    }
    #[derive(Debug)]
    struct RotatingProvider(Arc<AtomicUsize>);
    impl ProvideCredentials for RotatingProvider {
        fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
        where
            Self: 'a,
        {
            let index = self.0.fetch_add(1, Ordering::SeqCst);
            future::ProvideCredentials::ready(Ok(Credentials::new(
                format!("rotating-{index}"),
                format!("secret-{index}"),
                None,
                None,
                "post-test",
            )))
        }
    }
    #[tokio::test]
    async fn rotated_credentials_never_mix_a_scope_with_another_secret() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut store = store(RotatingProvider(Arc::clone(&calls)), "http://127.0.0.1:9");
        store.client = Client::from_conf(
            store
                .client
                .config()
                .to_builder()
                .identity_cache(aws_sdk_s3::config::IdentityCache::no_cache())
                .build(),
        );
        for index in 0..2 {
            let grant = store.presign_upload("key", &options()).await.unwrap();
            let scope: Vec<_> = grant.fields["x-amz-credential"].split('/').collect();
            assert_eq!(scope[0], format!("rotating-{index}"));
            assert_eq!(
                grant.fields["x-amz-signature"],
                sign_policy(
                    &format!("secret-{index}"),
                    scope[1],
                    scope[2],
                    &grant.fields["policy"]
                )
                .unwrap()
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn expired_credentials_and_non_bucket_signers_fail_closed() {
        let expired = Credentials::new(
            "post-test-access",
            "post-test-secret-only",
            None,
            Some(SystemTime::now() - Duration::from_secs(1)),
            "post-test",
        );
        assert!(
            store(expired, "http://127.0.0.1:9")
                .presign_upload("key", &options())
                .await
                .is_err()
        );
        for bucket in [
            "arn:aws:s3:us-east-1:123456789012:accesspoint/owned",
            "owned--usw2-az1--x-s3",
        ] {
            let mut store = store(BlockedProvider, "http://127.0.0.1:9");
            store.config.bucket = bucket.into();
            assert_eq!(
                store
                    .presign_upload("key", &options())
                    .await
                    .unwrap_err()
                    .code(),
                "s3_presign_invalid"
            );
        }
    }
}

#[cfg(test)]
mod conformance_tests {
    use super::*;
    use crate::{AsyncObjectStore, S3ObjectStoreConfig, aws::AwsS3Options};
    use aws_sdk_s3::{
        Client,
        config::{Credentials, Region, retry::RetryConfig},
    };

    #[test]
    fn signing_matches_the_published_aws_post_policy_vector() {
        // Public fictitious credentials and exact CRLF-encoded test data:
        // https://docs.aws.amazon.com/AmazonS3/latest/API/sigv4-post-example.html
        const POLICY: &str = "eyAiZXhwaXJhdGlvbiI6ICIyMDE1LTEyLTMwVDEyOjAwOjAwLjAwMFoiLA0KICAiY29uZGl0aW9ucyI6IFsNCiAgICB7ImJ1Y2tldCI6ICJzaWd2NGV4YW1wbGVidWNrZXQifSwNCiAgICBbInN0YXJ0cy13aXRoIiwgIiRrZXkiLCAidXNlci91c2VyMS8iXSwNCiAgICB7ImFjbCI6ICJwdWJsaWMtcmVhZCJ9LA0KICAgIHsic3VjY2Vzc19hY3Rpb25fcmVkaXJlY3QiOiAiaHR0cDovL3NpZ3Y0ZXhhbXBsZWJ1Y2tldC5zMy5hbWF6b25hd3MuY29tL3N1Y2Nlc3NmdWxfdXBsb2FkLmh0bWwifSwNCiAgICBbInN0YXJ0cy13aXRoIiwgIiRDb250ZW50LVR5cGUiLCAiaW1hZ2UvIl0sDQogICAgeyJ4LWFtei1tZXRhLXV1aWQiOiAiMTQzNjUxMjM2NTEyNzQifSwNCiAgICB7IngtYW16LXNlcnZlci1zaWRlLWVuY3J5cHRpb24iOiAiQUVTMjU2In0sDQogICAgWyJzdGFydHMtd2l0aCIsICIkeC1hbXotbWV0YS10YWciLCAiIl0sDQoNCiAgICB7IngtYW16LWNyZWRlbnRpYWwiOiAiQUtJQUlPU0ZPRE5ON0VYQU1QTEUvMjAxNTEyMjkvdXMtZWFzdC0xL3MzL2F3czRfcmVxdWVzdCJ9LA0KICAgIHsieC1hbXotYWxnb3JpdGhtIjogIkFXUzQtSE1BQy1TSEEyNTYifSwNCiAgICB7IngtYW16LWRhdGUiOiAiMjAxNTEyMjlUMDAwMDAwWiIgfQ0KICBdDQp9";
        assert_eq!(
            sign_policy(
                "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
                "20151229",
                "us-east-1",
                POLICY
            )
            .unwrap(),
            "8afdbf4008c03f22c2cd3cdb72e4afbb1f6a588f3255ac628749a66d7f09699e"
        );
    }

    async fn upload(
        grant: &PresignedUpload,
        body: Vec<u8>,
        mutate: impl FnOnce(&mut BTreeMap<String, String>),
    ) -> u16 {
        let mut fields = grant.fields.clone();
        mutate(&mut fields);
        let mut form = reqwest::multipart::Form::new();
        for (key, value) in fields {
            form = form.text(key, value);
        }
        form = form.part(
            "file",
            reqwest::multipart::Part::bytes(body)
                .file_name("file.txt")
                .mime_str("text/plain")
                .unwrap(),
        );
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap()
            .post(&grant.url)
            .multipart(form)
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    #[tokio::test]
    #[ignore = "requires isolated S3-compatible endpoint in FORKLAUNCH_TEST_S3_ENDPOINT"]
    #[allow(clippy::too_many_lines)] // One owned bucket lifecycle covers accepted and rejected grants.
    async fn s3_backend_post_policies_enforce_key_type_size_signature_and_expiration() {
        let endpoint = std::env::var("FORKLAUNCH_TEST_S3_ENDPOINT").unwrap();
        let client = Client::from_conf(
            aws_sdk_s3::Config::builder()
                .region(Region::new("us-east-1"))
                .credentials_provider(Credentials::new(
                    std::env::var("FORKLAUNCH_TEST_S3_ACCESS_KEY")
                        .unwrap_or_else(|_| "parity-test-access".into()),
                    std::env::var("FORKLAUNCH_TEST_S3_SECRET_KEY")
                        .unwrap_or_else(|_| "parity-test-secret-only".into()),
                    None,
                    None,
                    "post-test",
                ))
                .endpoint_url(endpoint)
                .force_path_style(true)
                .retry_config(RetryConfig::disabled())
                .behavior_version_latest()
                .build(),
        );
        let bucket = format!(
            "post-parity-{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let store = AwsS3ObjectStore::new(
            client.clone(),
            S3ObjectStoreConfig::new(&bucket),
            AwsS3Options {
                prefix: "tenant/owned".into(),
                ..AwsS3Options::default()
            },
        )
        .unwrap();
        store.init_bucket(true).await.unwrap();
        let key = "browser/é file.txt";
        let options = PresignUploadOptions {
            content_type: "text/plain".into(),
            max_bytes: 32,
            expires_in: Some(Duration::from_secs(60)),
        };
        let grant = store.presign_upload(key, &options).await.unwrap();
        assert_eq!(upload(&grant, vec![b'a'], |_| {}).await, 204);
        assert_eq!(store.get(key).await.unwrap().unwrap().body, vec![b'a']);
        assert_eq!(upload(&grant, vec![b'b'; 32], |_| {}).await, 204);
        assert_eq!(store.get(key).await.unwrap().unwrap().body, vec![b'b'; 32]);
        for body in [Vec::new(), vec![0; 33]] {
            assert!(matches!(upload(&grant, body, |_| {}).await, 400 | 403));
        }
        assert_eq!(
            upload(&grant, vec![b'a'], |fields| {
                fields.insert("key".into(), "tenant/other/escape".into());
            })
            .await,
            403
        );
        assert_eq!(
            upload(&grant, vec![b'a'], |fields| {
                fields.insert("Content-Type".into(), "application/json".into());
            })
            .await,
            403
        );
        assert_eq!(
            upload(&grant, vec![b'a'], |fields| {
                fields.insert("x-amz-signature".into(), "0".repeat(64));
            })
            .await,
            403
        );
        assert_eq!(
            upload(&grant, vec![b'a'], |fields| {
                let mut policy: serde_json::Value =
                    serde_json::from_slice(&STANDARD.decode(&fields["policy"]).unwrap()).unwrap();
                policy["conditions"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|condition| condition.get(0) != Some(&json!("content-length-range")));
                fields.insert(
                    "policy".into(),
                    STANDARD.encode(serde_json::to_vec(&policy).unwrap()),
                );
            })
            .await,
            403
        );
        assert_eq!(
            upload(&grant, vec![b'a'], |fields| {
                fields.insert("x-amz-meta-undeclared".into(), "injected".into());
            })
            .await,
            403
        );
        assert_eq!(
            store.get(key).await.unwrap().unwrap().body,
            vec![b'b'; 32],
            "rejected grants must not replace an object"
        );
        assert!(
            client
                .get_object()
                .bucket(&bucket)
                .key("tenant/other/escape")
                .send()
                .await
                .is_err()
        );
        let expiring = store
            .presign_upload(
                "expired",
                &PresignUploadOptions {
                    expires_in: Some(Duration::from_secs(1)),
                    ..options
                },
            )
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert_eq!(upload(&expiring, vec![b'a'], |_| {}).await, 403);
        assert!(store.get("expired").await.unwrap().is_none());
        store.delete(key).await.unwrap();
        client.delete_bucket().bucket(bucket).send().await.unwrap();
    }
}

#[cfg(test)]
mod typescript_parity_tests {
    use super::*;
    use crate::{S3ObjectStoreConfig, aws::AwsS3Options};
    use aws_sdk_s3::{Client, config::Region};

    #[tokio::test]
    #[ignore = "requires actual TypeScript fixtures from framework-rs/parity/rust_s3_post_parity.mjs"]
    async fn post_policy_matches_actual_typescript() {
        let fixtures: serde_json::Value = serde_json::from_slice(
            &std::fs::read(std::env::var("FORKLAUNCH_TEST_S3_POST_FIXTURES").unwrap()).unwrap(),
        )
        .unwrap();
        for fixture in fixtures.as_array().unwrap() {
            let client = Client::from_conf(
                aws_sdk_s3::Config::builder()
                    .region(Region::new("us-east-1"))
                    .credentials_provider(Credentials::new(
                        "post-test-access",
                        "post-test-secret-only",
                        None,
                        None,
                        "post-test",
                    ))
                    .endpoint_url("http://127.0.0.1:9")
                    .force_path_style(true)
                    .behavior_version_latest()
                    .build(),
            );
            let store = AwsS3ObjectStore::new(
                client,
                S3ObjectStoreConfig::new("post-parity"),
                AwsS3Options {
                    prefix: fixture["prefix"].as_str().unwrap().into(),
                    ..AwsS3Options::default()
                },
            )
            .unwrap();
            let key = fixture["key"].as_str().unwrap();
            let options = PresignUploadOptions {
                content_type: fixture["contentType"].as_str().unwrap().into(),
                max_bytes: fixture["maxBytes"].as_u64().unwrap_or_default(),
                expires_in: fixture
                    .get("expiresIn")
                    .map(|seconds| Duration::from_secs_f64(seconds.as_f64().unwrap())),
            };
            let grant = store.presign_upload(key, &options).await;
            if fixture["accepted"] == false {
                assert!(grant.is_err(), "{fixture}");
                continue;
            }
            let grant = grant.unwrap();
            assert_eq!(grant.key, key);
            assert_eq!(grant.fields["key"], fixture["wireKey"]);
            assert_eq!(
                grant.fields["Content-Type"],
                fixture["fields"]["Content-Type"]
            );
            let policy: serde_json::Value =
                serde_json::from_slice(&STANDARD.decode(&grant.fields["policy"]).unwrap()).unwrap();
            let conditions = policy["conditions"].as_array().unwrap();
            assert!(conditions.contains(&json!(["content-length-range", 1, fixture["maxBytes"]])));
            assert!(conditions.contains(&json!({"Content-Type":fixture["contentType"]})));
            assert!(conditions.contains(&json!({"key":fixture["wireKey"]})));
            let remaining = grant.expires_at.duration_since(SystemTime::now()).unwrap();
            let expected = fixture["seconds"].as_u64().unwrap();
            assert!(remaining <= Duration::from_secs(expected));
            assert!(remaining >= Duration::from_millis(expected * 1000 - 100));
        }
    }
}
