//! Async Amazon S3 adapter using an injected official AWS SDK client.
#[path = "aws_environment.rs"]
pub mod environment;
#[path = "aws_post.rs"]
pub mod post;
#[path = "aws_transfer.rs"]
pub mod transfer;
pub use post::{PresignUploadOptions, PresignedUpload};
use std::{future::Future, time::Duration};

use super::{AsyncObjectStore, ObjectMetadata, S3ObjectStoreConfig, StoredObject, normalize_key};
pub use aws_sdk_s3;
use aws_sdk_s3::{
    Client,
    presigning::PresigningConfig,
    primitives::ByteStream,
    types::{BucketLocationConstraint, CreateBucketConfiguration},
};
use forklaunch_core::common::{FrameworkError, FrameworkResult};

/// Resource bounds and a namespace applied to every operation.
#[derive(Debug, Clone)]
pub struct AwsS3Options {
    pub prefix: String,
    pub max_object_bytes: usize,
    pub max_list_bytes: usize,
    pub max_list_objects: usize,
    pub operation_timeout: Duration,
    pub max_download_lifetime: Duration,
    pub max_upload_lifetime: Duration,
}
impl Default for AwsS3Options {
    fn default() -> Self {
        Self {
            prefix: String::new(),
            max_object_bytes: 16 * 1024 * 1024,
            max_list_bytes: 64 * 1024 * 1024,
            max_list_objects: 1000,
            operation_timeout: Duration::from_secs(30),
            max_download_lifetime: Duration::from_secs(300),
            max_upload_lifetime: Duration::from_mins(15),
        }
    }
}

/// Optional lifetime and attachment filename for one presigned download.
#[derive(Debug, Clone, Default)]
pub struct PresignDownloadOptions {
    pub expires_in: Option<Duration>,
    pub filename: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectPage {
    pub objects: Vec<StoredObject>,
    pub next_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AwsS3ObjectStore {
    client: Client,
    config: S3ObjectStoreConfig,
    options: AwsS3Options,
}
impl AwsS3ObjectStore {
    /// The caller supplies credentials, endpoint, TLS and retry policy through `client`.
    /// Construction makes no network requests and never creates a bucket.
    /// # Errors
    /// Rejects invalid configuration, namespace prefixes, and zero resource bounds.
    pub fn new(
        client: Client,
        config: S3ObjectStoreConfig,
        mut options: AwsS3Options,
    ) -> FrameworkResult<Self> {
        config.validate()?;
        if options.max_object_bytes == 0
            || options.max_list_bytes == 0
            || options.max_list_objects == 0
            || options.operation_timeout.is_zero()
            || options.max_download_lifetime.as_secs() == 0
            || options.max_download_lifetime.subsec_nanos() != 0
            || options.max_download_lifetime > Duration::from_hours(168)
            || options.max_upload_lifetime.as_secs() == 0
            || options.max_upload_lifetime.subsec_nanos() != 0
            || options.max_upload_lifetime > Duration::from_hours(168)
        {
            return Err(error(
                "s3_config_invalid",
                "object, listing and timeout limits must be positive",
            ));
        }
        if !options.prefix.is_empty() {
            options.prefix = format!("{}/", normalize_key(&options.prefix)?.trim_end_matches('/'));
            if options.prefix.len() >= 1024 {
                return Err(error("s3_config_invalid", "namespace prefix is too long"));
            }
        }
        Ok(Self {
            client,
            config,
            options,
        })
    }

    #[must_use]
    pub fn config(&self) -> &S3ObjectStoreConfig {
        &self.config
    }

    fn bounded<T>(
        &self,
        future: impl Future<Output = FrameworkResult<T>>,
    ) -> impl Future<Output = FrameworkResult<T>> {
        let deadline = tokio::time::timeout(self.options.operation_timeout, Box::pin(future));
        async move {
            deadline
                .await
                .map_err(|_| error("s3_timeout", "object store operation exceeded its deadline"))?
        }
    }

    fn wire_key(&self, key: &str) -> FrameworkResult<(String, String)> {
        let key = normalize_key(key)?;
        let wire = format!("{}{key}", self.options.prefix);
        if wire.len() > 1024 {
            return Err(error(
                "s3_object_invalid_key",
                "object key exceeds 1024 UTF-8 bytes",
            ));
        }
        Ok((key, wire))
    }

    /// Checks bucket access and optionally creates an absent bucket. Permission,
    /// credential and transport failures never trigger creation.
    /// # Errors
    /// Returns an error on inaccessible buckets, failed creation, or timeout.
    pub async fn init_bucket(&self, create_if_missing: bool) -> FrameworkResult<()> {
        self.bounded(async {
            match self
                .client
                .head_bucket()
                .bucket(&self.config.bucket)
                .send()
                .await
            {
                Ok(_) => return Ok(()),
                Err(failure)
                    if failure
                        .as_service_error()
                        .is_some_and(aws_sdk_s3::operation::head_bucket::HeadBucketError::is_not_found)
                        || failure
                            .raw_response()
                            .is_some_and(|response| response.status().as_u16() == 404) =>
                {
                    if !create_if_missing {
                        return Err(error(
                            "s3_bucket_missing",
                            "configured bucket does not exist",
                        ));
                    }
                }
                Err(_) => return Err(request_error()),
            }
            let mut request = self.client.create_bucket().bucket(&self.config.bucket);
            let region = self
                .config
                .region
                .as_deref()
                .or_else(|| self.client.config().region().map(aws_sdk_s3::config::Region::as_ref));
            if let Some(region) = region.filter(|region| *region != "us-east-1") {
                request = request.create_bucket_configuration(
                    CreateBucketConfiguration::builder()
                        .location_constraint(BucketLocationConstraint::from(region))
                        .build(),
                );
            }
            match request.send().await {
                Ok(_) => Ok(()),
                Err(failure)
                    if failure
                        .as_service_error()
                        .is_some_and(aws_sdk_s3::operation::create_bucket::CreateBucketError::is_bucket_already_owned_by_you) =>
                {
                    Ok(())
                }
                Err(_) => Err(request_error()),
            }
        })
        .await
    }

    /// Signs a namespaced GET using the injected SDK credential provider. This
    /// creates no bucket and makes no S3 object request. Credential resolution
    /// remains subject to the operation deadline. Treat the URL as a bearer grant.
    /// # Errors
    /// Rejects invalid keys/filenames/lifetimes and signing or credential failures.
    pub async fn presign_download(
        &self,
        key: &str,
        options: &PresignDownloadOptions,
    ) -> FrameworkResult<String> {
        self.bounded(async {
            let (_, wire) = self.wire_key(key)?;
            let lifetime = options
                .expires_in
                .unwrap_or(self.options.max_download_lifetime);
            if lifetime.as_secs() == 0
                || lifetime.subsec_nanos() != 0
                || lifetime > self.options.max_download_lifetime
            {
                return Err(error(
                    "s3_presign_invalid",
                    "download lifetime must be whole positive seconds within its configured cap",
                ));
            }
            let mut request = self
                .client
                .get_object()
                .bucket(&self.config.bucket)
                .key(wire);
            if let Some(filename) = options.filename.as_deref().filter(|name| !name.is_empty()) {
                if filename.len() > 1024 {
                    return Err(error(
                        "s3_presign_invalid",
                        "download filename exceeds 1024 UTF-8 bytes",
                    ));
                }
                request = request.response_content_disposition(content_disposition(filename));
            }
            let signing = PresigningConfig::expires_in(lifetime)
                .map_err(|_| error("s3_presign_invalid", "invalid download lifetime"))?;
            let signed = request
                .presigned(signing)
                .await
                .map_err(|_| request_error())?;
            Ok(signed.uri().to_owned())
        })
        .await
    }

    async fn get_inner(&self, key: &str) -> FrameworkResult<Option<StoredObject>> {
        let Some(mut download) = self.download_inner(key).await? else {
            return Ok(None);
        };
        let mut body = Vec::new();
        while let Some(chunk) = download.next_chunk().await? {
            body.extend_from_slice(&chunk);
        }
        Ok(Some(StoredObject {
            key: download.key,
            body,
            metadata: download.metadata,
        }))
    }

    /// Returns one bounded page. Continuation tokens are opaque and scoped to the
    /// same bucket/namespace/prefix; concurrent changes are not snapshot isolated.
    /// # Errors
    /// Rejects invalid limits, prefixes, backend failures, and resource overflow.
    pub async fn list_page(
        &self,
        prefix: &str,
        token: Option<&str>,
        limit: usize,
    ) -> FrameworkResult<ObjectPage> {
        self.bounded(self.list_page_inner(prefix, token, limit))
            .await
    }

    async fn list_page_inner(
        &self,
        prefix: &str,
        token: Option<&str>,
        limit: usize,
    ) -> FrameworkResult<ObjectPage> {
        if prefix.contains("..")
            || !(1..=1000).contains(&limit)
            || limit > self.options.max_list_objects
        {
            return Err(error("s3_list_invalid", "invalid prefix or page size"));
        }
        let wire_prefix = format!("{}{prefix}", self.options.prefix);
        let output = self
            .client
            .list_objects_v2()
            .bucket(&self.config.bucket)
            .prefix(&wire_prefix)
            .set_continuation_token(token.map(str::to_owned))
            .max_keys(
                i32::try_from(limit).map_err(|_| error("s3_list_invalid", "invalid page size"))?,
            )
            .send()
            .await
            .map_err(|_| request_error())?;
        if output.contents().len() > limit {
            return Err(error(
                "s3_list_limit",
                "object listing exceeds configured bounds",
            ));
        }
        let next_token = output.next_continuation_token().map(str::to_owned);
        if output.is_truncated() == Some(true) && next_token.is_none() {
            return Err(error(
                "s3_list_invalid",
                "truncated listing has no continuation token",
            ));
        }
        let mut objects = Vec::new();
        let mut bytes = 0usize;
        for object in output.contents() {
            let wire = object
                .key()
                .filter(|key| key.starts_with(&wire_prefix))
                .ok_or_else(|| error("s3_list_invalid", "object listing escaped its prefix"))?;
            let key = wire
                .strip_prefix(&self.options.prefix)
                .ok_or_else(|| error("s3_list_invalid", "object listing escaped its namespace"))?;
            if let Some(object) = self.get_inner(key).await? {
                bytes = bytes
                    .checked_add(object.body.len())
                    .filter(|bytes| *bytes <= self.options.max_list_bytes)
                    .ok_or_else(|| {
                        error(
                            "s3_list_limit",
                            "object listing exceeds configured byte bound",
                        )
                    })?;
                objects.push(object);
            }
        }
        Ok(ObjectPage {
            objects,
            next_token,
        })
    }
}

impl AsyncObjectStore for AwsS3ObjectStore {
    async fn put(
        &self,
        key: &str,
        body: Vec<u8>,
        metadata: ObjectMetadata,
    ) -> FrameworkResult<StoredObject> {
        self.bounded(async {
            let (key, wire) = self.wire_key(key)?;
            if body.len() > self.options.max_object_bytes {
                return Err(size_error());
            }
            let (metadata, tagging) = upload_metadata(metadata)?;
            self.client
                .put_object()
                .bucket(&self.config.bucket)
                .key(wire)
                .body(ByteStream::from(body.clone()))
                .set_content_type(metadata.content_type.clone())
                .set_tagging(tagging)
                .send()
                .await
                .map_err(|_| request_error())?;
            Ok(StoredObject {
                key,
                body,
                metadata,
            })
        })
        .await
    }

    async fn get(&self, key: &str) -> FrameworkResult<Option<StoredObject>> {
        self.bounded(self.get_inner(key)).await
    }

    /// Existence is checked before deletion; the returned boolean is advisory
    /// under concurrent writes/deletes. S3 deletion itself is idempotent.
    async fn delete(&self, key: &str) -> FrameworkResult<bool> {
        self.bounded(async {
            let (_, wire) = self.wire_key(key)?;
            match self
                .client
                .head_object()
                .bucket(&self.config.bucket)
                .key(&wire)
                .send()
                .await
            {
                Ok(_) => {}
                Err(failure)
                    if failure.as_service_error().is_some_and(
                        aws_sdk_s3::operation::head_object::HeadObjectError::is_not_found,
                    ) =>
                {
                    return Ok(false);
                }
                Err(_) => return Err(request_error()),
            }
            self.client
                .delete_object()
                .bucket(&self.config.bucket)
                .key(wire)
                .send()
                .await
                .map_err(|_| request_error())?;
            Ok(true)
        })
        .await
    }

    async fn list(&self, prefix: &str) -> FrameworkResult<Vec<StoredObject>> {
        self.bounded(async {
            let mut objects = Vec::new();
            let mut token = None;
            let mut bytes = 0usize;
            loop {
                let remaining = self.options.max_list_objects.saturating_sub(objects.len());
                if remaining == 0 {
                    return Err(error(
                        "s3_list_limit",
                        "object listing exceeds configured count bound",
                    ));
                }
                let page = self
                    .list_page_inner(prefix, token.as_deref(), remaining.min(1000))
                    .await?;
                for object in page.objects {
                    bytes = bytes
                        .checked_add(object.body.len())
                        .filter(|bytes| *bytes <= self.options.max_list_bytes)
                        .ok_or_else(|| {
                            error(
                                "s3_list_limit",
                                "object listing exceeds configured byte bound",
                            )
                        })?;
                    objects.push(object);
                }
                let Some(next) = page.next_token else {
                    return Ok(objects);
                };
                if token.as_ref() == Some(&next) {
                    return Err(error(
                        "s3_list_invalid",
                        "object listing repeated its continuation token",
                    ));
                }
                token = Some(next);
            }
        })
        .await
    }
}

fn error(code: &str, message: &str) -> FrameworkError {
    FrameworkError::new(code, message)
}
fn request_error() -> FrameworkError {
    error("s3_request_failed", "object store request failed")
}
fn size_error() -> FrameworkError {
    error(
        "s3_object_too_large",
        "object exceeds configured byte bound",
    )
}
fn upload_metadata(
    mut metadata: ObjectMetadata,
) -> FrameworkResult<(ObjectMetadata, Option<String>)> {
    metadata
        .content_type
        .get_or_insert_with(|| "application/octet-stream".into());
    if metadata.tags.len() > 10 {
        return Err(error(
            "s3_metadata_invalid",
            "S3 accepts at most ten object tags",
        ));
    }
    let tags = metadata
        .tags
        .iter()
        .map(|(key, value)| format!("{}={}", encode_tag(key), encode_tag(value)))
        .collect::<Vec<_>>()
        .join("&");
    Ok((metadata, (!tags.is_empty()).then_some(tags)))
}

fn content_disposition(filename: &str) -> String {
    let ascii = filename
        .encode_utf16()
        .map(|unit| {
            if (0x20..=0x7e).contains(&unit) && ![34, 92].contains(&unit) {
                char::from_u32(u32::from(unit)).unwrap_or('_')
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!(
        "attachment; filename=\"{ascii}\"; filename*=UTF-8''{}",
        encode_tag(filename)
    )
}

fn encode_tag(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_s3::config::{Credentials, Region, retry::RetryConfig};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn client(endpoint: &str) -> Client {
        Client::from_conf(
            aws_sdk_s3::Config::builder()
                .region(Region::new("us-east-1"))
                .credentials_provider(Credentials::new(
                    std::env::var("FORKLAUNCH_TEST_S3_ACCESS_KEY")
                        .unwrap_or_else(|_| "parity-test-access".into()),
                    std::env::var("FORKLAUNCH_TEST_S3_SECRET_KEY")
                        .unwrap_or_else(|_| "parity-test-secret-only".into()),
                    None,
                    None,
                    "parity-test",
                ))
                .endpoint_url(endpoint)
                .force_path_style(true)
                .retry_config(RetryConfig::standard().with_max_attempts(1))
                .build(),
        )
    }
    fn test_store(client: Client, prefix: &str, mut options: AwsS3Options) -> AwsS3ObjectStore {
        options.prefix = prefix.into();
        AwsS3ObjectStore::new(
            client,
            S3ObjectStoreConfig::new("forklaunch-parity").with_region("us-east-1"),
            options,
        )
        .unwrap()
    }

    #[test]
    fn aws_config_and_keys_reject_invalid_bounds_without_network_access() {
        let client = client("http://127.0.0.1:1");
        for options in [
            AwsS3Options {
                max_upload_lifetime: Duration::ZERO,
                ..AwsS3Options::default()
            },
            AwsS3Options {
                max_upload_lifetime: Duration::from_millis(1500),
                ..AwsS3Options::default()
            },
            AwsS3Options {
                max_upload_lifetime: Duration::from_secs(604_801),
                ..AwsS3Options::default()
            },
            AwsS3Options {
                max_object_bytes: 0,
                ..AwsS3Options::default()
            },
            AwsS3Options {
                max_list_bytes: 0,
                ..AwsS3Options::default()
            },
            AwsS3Options {
                operation_timeout: Duration::ZERO,
                ..AwsS3Options::default()
            },
            AwsS3Options {
                prefix: "../other".into(),
                ..AwsS3Options::default()
            },
            AwsS3Options {
                prefix: "/".into(),
                ..AwsS3Options::default()
            },
        ] {
            assert!(
                AwsS3ObjectStore::new(client.clone(), S3ObjectStoreConfig::new("assets"), options)
                    .is_err()
            );
        }
        let store = test_store(client, "tenant", AwsS3Options::default());
        for key in ["", " ", "/", "///", "../other"] {
            assert!(store.wire_key(key).is_err());
        }
        assert!(store.wire_key(&"a".repeat(1024)).is_err());
        assert_eq!(
            store.wire_key("/a/🦀").unwrap(),
            ("a/🦀".into(), "tenant/a/🦀".into())
        );
        assert_eq!(encode_tag("hello /é+"), "hello%20%2F%C3%A9%2B");
    }

    #[tokio::test]
    async fn aws_download_lifetimes_and_filenames_are_bounded_without_storage_requests() {
        let store = test_store(
            client("http://127.0.0.1:1"),
            "tenant",
            AwsS3Options::default(),
        );
        for lifetime in [
            Duration::ZERO,
            Duration::from_secs(301),
            Duration::from_millis(1500),
        ] {
            let options = PresignDownloadOptions {
                expires_in: Some(lifetime),
                filename: None,
            };
            assert_eq!(
                store
                    .presign_download("key", &options)
                    .await
                    .unwrap_err()
                    .code(),
                "s3_presign_invalid"
            );
        }
        let options = PresignDownloadOptions {
            filename: Some("x".repeat(1025)),
            ..PresignDownloadOptions::default()
        };
        assert!(store.presign_download("key", &options).await.is_err());
        assert!(
            store
                .presign_download("/", &PresignDownloadOptions::default())
                .await
                .is_err()
        );
        let url = store
            .presign_download("a/é file", &PresignDownloadOptions::default())
            .await
            .unwrap();
        let url = reqwest::Url::parse(&url).unwrap();
        assert!(
            url.query_pairs()
                .any(|(key, value)| key == "X-Amz-Expires" && value == "300")
        );
        assert!(url.path().starts_with("/forklaunch-parity/tenant/a/"));
        let disposition = content_disposition("report \"é\" 🦀\r\n.bin");
        assert!(!disposition.contains(['\r', '\n']));
        assert!(
            disposition.contains("filename*=UTF-8''report%20%22%C3%A9%22%20%F0%9F%A6%80%0D%0A.bin")
        );
    }

    #[tokio::test]
    async fn aws_operations_stop_at_the_configured_deadline() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (_connection, _) = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
        });
        let store = test_store(
            client(&endpoint),
            "deadline",
            AwsS3Options {
                operation_timeout: Duration::from_millis(100),
                ..AwsS3Options::default()
            },
        );
        let error = store.get("blocked").await.unwrap_err();
        assert_eq!(error.code(), "s3_timeout");
        server.abort();
    }

    #[tokio::test]
    async fn aws_bucket_creation_does_not_follow_access_denied() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(AtomicUsize::new(0));
        let seen = requests.clone();
        let server = tokio::spawn(async move {
            loop {
                let (mut connection, _) = listener.accept().await.unwrap();
                let mut request = [0; 8192];
                let _ = connection.read(&mut request).await.unwrap();
                seen.fetch_add(1, Ordering::SeqCst);
                connection
                    .write_all(
                        b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await
                    .unwrap();
            }
        });
        let store = test_store(client(&endpoint), "denied", AwsS3Options::default());
        assert_eq!(
            store.init_bucket(true).await.unwrap_err().code(),
            "s3_request_failed"
        );
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        server.abort();
    }

    #[tokio::test]
    #[ignore = "requires isolated S3-compatible endpoint in FORKLAUNCH_TEST_S3_ENDPOINT"]
    async fn s3_backend_round_trips_binary_tags_batches_namespaces_and_pages() {
        let endpoint = std::env::var("FORKLAUNCH_TEST_S3_ENDPOINT")
            .expect("FORKLAUNCH_TEST_S3_ENDPOINT must be set");
        let client = client(&endpoint);
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let missing = AwsS3ObjectStore::new(
            client.clone(),
            S3ObjectStoreConfig::new(format!("missing-{id}")),
            AwsS3Options::default(),
        )
        .unwrap();
        assert_eq!(
            missing.init_bucket(false).await.unwrap_err().code(),
            "s3_bucket_missing"
        );
        assert_eq!(
            missing.get("absent").await.unwrap_err().code(),
            "s3_request_failed"
        );
        let prefix = format!("test-{id}/tenant-a");
        let store = test_store(client.clone(), &prefix, AwsS3Options::default());
        store.init_bucket(true).await.unwrap();
        store.init_bucket(false).await.unwrap();
        let foreign = test_store(
            client.clone(),
            &format!("test-{id}/tenant-b"),
            AwsS3Options::default(),
        );
        foreign
            .put("records/foreign", vec![99], ObjectMetadata::new())
            .await
            .unwrap();
        assert_empty_and_codec_roundtrips(&store).await;
        assert_presigned_downloads(&store).await;
        assert_native_transfers(&store, client.clone(), &prefix).await;
        assert_multipart_transfers(&store).await;
        let metadata = ObjectMetadata::new()
            .with_content_type("application/octet-stream")
            .with_tag("service", "accounts")
            .with_tag("label", "hello rust/cloud");
        let objects = vec![
            (
                "records/a".into(),
                vec![0, 255, 13, 10, 42],
                metadata.clone(),
            ),
            ("records/b".into(), vec![1, 2], metadata.clone()),
            ("records/c".into(), vec![3, 4], metadata.clone()),
        ];
        let stored = store.put_batch(objects).await.unwrap();
        assert_eq!(
            store.get("/records/a").await.unwrap().as_ref(),
            Some(&stored[0])
        );
        let read = store
            .get_batch(&["records/c".into(), "absent".into(), "records/a".into()])
            .await
            .unwrap();
        assert_eq!(
            read,
            [Some(stored[2].clone()), None, Some(stored[0].clone())]
        );
        let first = store.list_page("records/", None, 2).await.unwrap();
        assert_eq!(first.objects, stored[..2]);
        let second = store
            .list_page("records/", first.next_token.as_deref(), 2)
            .await
            .unwrap();
        assert_eq!(second.objects, stored[2..]);
        assert!(second.next_token.is_none());
        assert_eq!(store.list("records/").await.unwrap(), stored);
        assert!(store.list("records/../").await.is_err());
        assert!(store.list_page("", None, 0).await.is_err());
        assert_resource_bounds(client, &prefix, &store).await;
        assert_eq!(
            store
                .delete_batch(&["records/a".into(), "absent".into(), "records/b".into()])
                .await
                .unwrap(),
            2
        );
        assert!(!store.delete("records/a").await.unwrap());
        assert_eq!(store.list("records/").await.unwrap(), stored[2..]);
        assert!(store.delete("records/c").await.unwrap());
        assert!(store.list("").await.unwrap().is_empty());
        assert!(foreign.get("records/foreign").await.unwrap().is_some());
        foreign.delete("records/foreign").await.unwrap();
    }
    async fn assert_resource_bounds(client: Client, prefix: &str, store: &AwsS3ObjectStore) {
        let tiny = test_store(
            client.clone(),
            prefix,
            AwsS3Options {
                max_object_bytes: 4,
                ..AwsS3Options::default()
            },
        );
        assert_eq!(
            tiny.get("records/a").await.unwrap_err().code(),
            "s3_object_too_large"
        );
        assert_eq!(
            tiny.put("oversized", vec![0; 5], ObjectMetadata::new())
                .await
                .unwrap_err()
                .code(),
            "s3_object_too_large"
        );
        assert!(store.get("oversized").await.unwrap().is_none());
        let count_bound = test_store(
            client.clone(),
            prefix,
            AwsS3Options {
                max_list_objects: 2,
                ..AwsS3Options::default()
            },
        );
        assert_eq!(
            count_bound.list("records/").await.unwrap_err().code(),
            "s3_list_limit"
        );
        let byte_bound = test_store(
            client,
            prefix,
            AwsS3Options {
                max_list_bytes: 3,
                ..AwsS3Options::default()
            },
        );
        assert_eq!(
            byte_bound.list("records/").await.unwrap_err().code(),
            "s3_list_limit"
        );
    }
    async fn assert_empty_and_codec_roundtrips(store: &AwsS3ObjectStore) {
        let empty = store
            .put("empty", vec![], ObjectMetadata::new())
            .await
            .unwrap();
        assert_eq!(store.get("empty").await.unwrap(), Some(empty));
        store.delete("empty").await.unwrap();
        // The codec hook is transport independent; production callers supply an AEAD codec.
        let codec = crate::XorObjectCodec::new(b"test-only-codec").unwrap();
        crate::put_encrypted_async(
            store,
            &codec,
            "encoded",
            b"plaintext",
            ObjectMetadata::new(),
        )
        .await
        .unwrap();
        assert_ne!(
            store.get("encoded").await.unwrap().unwrap().body,
            b"plaintext"
        );
        assert_eq!(
            crate::get_decrypted_async(store, &codec, "encoded")
                .await
                .unwrap(),
            Some(b"plaintext".to_vec())
        );
        store.delete("encoded").await.unwrap();
    }
    async fn assert_presigned_downloads(store: &AwsS3ObjectStore) {
        let key = "downloads/é space.bin";
        let body = vec![0, 255, 13, 10, 42];
        store
            .put(key, body.clone(), ObjectMetadata::new())
            .await
            .unwrap();
        let options = PresignDownloadOptions {
            filename: Some("report \"é\" 🦀.bin".into()),
            ..PresignDownloadOptions::default()
        };
        let url = store.presign_download(key, &options).await.unwrap();
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let response = http
            .get(&url)
            .send()
            .await
            .unwrap_or_else(|_| panic!("presigned download transport failed"));
        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(
            response.headers().get("content-disposition").unwrap(),
            content_disposition(options.filename.as_deref().unwrap()).as_str()
        );
        assert_eq!(response.bytes().await.unwrap().as_ref(), body);
        let mut tampered = reqwest::Url::parse(&url).unwrap();
        tampered.set_path("/forklaunch-parity/other-namespace/foreign");
        assert_eq!(
            http.get(tampered)
                .send()
                .await
                .unwrap_or_else(|_| panic!("tamper transport failed"))
                .status()
                .as_u16(),
            403
        );
        let expired = store
            .presign_download(
                key,
                &PresignDownloadOptions {
                    expires_in: Some(Duration::from_secs(1)),
                    filename: None,
                },
            )
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(
            http.get(expired)
                .send()
                .await
                .unwrap_or_else(|_| panic!("expiry transport failed"))
                .status()
                .as_u16(),
            403
        );
        store.delete(key).await.unwrap();
    }
    struct MultipartSource {
        chunks: std::collections::VecDeque<bytes::Bytes>,
        fail: bool,
        pending: bool,
    }
    impl http_body::Body for MultipartSource {
        type Data = bytes::Bytes;
        type Error = std::io::Error;
        fn poll_frame(
            mut self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
            if let Some(chunk) = self.chunks.pop_front() {
                return std::task::Poll::Ready(Some(Ok(http_body::Frame::data(chunk))));
            }
            if self.pending {
                return std::task::Poll::Pending;
            }
            if std::mem::take(&mut self.fail) {
                return std::task::Poll::Ready(Some(Err(std::io::Error::other(
                    "fixture source failure",
                ))));
            }
            std::task::Poll::Ready(None)
        }
    }
    fn multipart_source(bytes: &[u8], fail: bool, pending: bool) -> ByteStream {
        ByteStream::new(aws_sdk_s3::primitives::SdkBody::from_body_1_x(
            MultipartSource {
                chunks: bytes
                    .chunks(131_071)
                    .map(bytes::Bytes::copy_from_slice)
                    .collect(),
                fail,
                pending,
            },
        ))
    }
    async fn assert_no_incomplete_uploads(store: &AwsS3ObjectStore) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let uploads = store
                .client
                .list_multipart_uploads()
                .bucket(&store.config.bucket)
                .prefix(&store.options.prefix)
                .send()
                .await
                .unwrap();
            if uploads.uploads().is_empty() {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "multipart cleanup did not finish"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
    async fn assert_multipart_transfers(store: &AwsS3ObjectStore) {
        let body = (0u8..=255)
            .cycle()
            .take(7 * 1024 * 1024 + 17)
            .collect::<Vec<_>>();
        let metadata = ObjectMetadata::new()
            .with_content_type("application/octet-stream")
            .with_tag("source", "multipart");
        let uploaded = store
            .put_multipart_stream(
                "multipart/data",
                multipart_source(&body, false, false),
                metadata.clone(),
            )
            .await
            .unwrap();
        assert_eq!(uploaded.content_length, body.len() as u64);
        assert!(uploaded.etag.is_some());
        let stored = store.get("multipart/data").await.unwrap().unwrap();
        assert_eq!(stored.body, body);
        assert_eq!(stored.metadata, metadata);
        for (key, bytes) in [
            ("multipart/empty", Vec::new()),
            ("multipart/exact", vec![3; 5 * 1024 * 1024]),
        ] {
            store
                .put_multipart_stream(
                    key,
                    multipart_source(&bytes, false, false),
                    ObjectMetadata::new(),
                )
                .await
                .unwrap();
            assert_eq!(store.get(key).await.unwrap().unwrap().body, bytes);
            store.delete(key).await.unwrap();
        }
        assert_eq!(
            store
                .put_multipart_stream(
                    "multipart/failure",
                    multipart_source(&body, true, false),
                    ObjectMetadata::new()
                )
                .await
                .unwrap_err()
                .code(),
            "s3_source_failed"
        );
        assert!(store.get("multipart/failure").await.unwrap().is_none());
        assert_no_incomplete_uploads(store).await;
        let mut tiny = store.clone();
        tiny.options.max_object_bytes = 4;
        assert_eq!(
            tiny.put_multipart_stream(
                "multipart/overflow",
                multipart_source(&body[..5], false, false),
                ObjectMetadata::new()
            )
            .await
            .unwrap_err()
            .code(),
            "s3_object_too_large"
        );
        assert!(store.get("multipart/overflow").await.unwrap().is_none());
        assert_no_incomplete_uploads(store).await;
        let mut short = store.clone();
        short.options.operation_timeout = Duration::from_millis(150);
        assert_eq!(
            short
                .put_multipart_stream(
                    "multipart/timeout",
                    multipart_source(&[], false, true),
                    ObjectMetadata::new()
                )
                .await
                .unwrap_err()
                .code(),
            "s3_timeout"
        );
        assert_no_incomplete_uploads(store).await;
        assert_multipart_cancellation(store).await;
        store.delete("multipart/data").await.unwrap();
    }

    async fn assert_multipart_cancellation(store: &AwsS3ObjectStore) {
        let owned = store.clone();
        let task = tokio::spawn(async move {
            owned
                .put_multipart_stream(
                    "multipart/cancelled",
                    multipart_source(&vec![7; 5 * 1024 * 1024], false, true),
                    ObjectMetadata::new(),
                )
                .await
        });
        // Wait for a real uploaded part before cancellation; cleanup must remove it.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let uploads = store
                .client
                .list_multipart_uploads()
                .bucket(&store.config.bucket)
                .prefix(format!("{}multipart/cancelled", store.options.prefix))
                .send()
                .await
                .unwrap();
            if let Some(upload) = uploads.uploads().first() {
                let parts = store
                    .client
                    .list_parts()
                    .bucket(&store.config.bucket)
                    .key(upload.key().unwrap())
                    .upload_id(upload.upload_id().unwrap())
                    .send()
                    .await
                    .unwrap();
                if !parts.parts().is_empty() {
                    break;
                }
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "multipart part was never uploaded"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_no_incomplete_uploads(store).await;
        assert!(store.get("multipart/cancelled").await.unwrap().is_none());
    }

    async fn assert_native_transfers(store: &AwsS3ObjectStore, client: Client, prefix: &str) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("upload.bin");
        let body = (0u8..=255)
            .cycle()
            .take(2 * 1024 * 1024)
            .collect::<Vec<_>>();
        tokio::fs::write(&path, &body).await.unwrap();
        let metadata = ObjectMetadata::new()
            .with_content_type("application/octet-stream")
            .with_tag("source", "file");
        let uploaded = store
            .put_file("native/file", &path, metadata.clone())
            .await
            .unwrap();
        assert_eq!(uploaded.key, "native/file");
        assert_eq!(uploaded.content_length, body.len() as u64);
        assert!(uploaded.etag.is_some());
        let mut download = store.download("native/file").await.unwrap().unwrap();
        assert_eq!(download.metadata, metadata);
        assert_eq!(download.content_length, Some(body.len() as u64));
        let mut read = Vec::new();
        while let Some(chunk) = download.next_chunk().await.unwrap() {
            read.extend_from_slice(&chunk);
        }
        assert_eq!(read, body);
        assert!(store.download("native/missing").await.unwrap().is_none());
        let mut abandoned = store.download("native/file").await.unwrap().unwrap();
        assert!(abandoned.next_chunk().await.unwrap().is_some());
        drop(abandoned);
        assert_eq!(store.get("native/file").await.unwrap().unwrap().body, body);
        let tiny = test_store(
            client,
            prefix,
            AwsS3Options {
                max_object_bytes: 4,
                ..AwsS3Options::default()
            },
        );
        assert_eq!(
            tiny.download("native/file").await.unwrap_err().code(),
            "s3_object_too_large"
        );
        assert_eq!(
            tiny.put_file("native/oversized", &path, ObjectMetadata::new())
                .await
                .unwrap_err()
                .code(),
            "s3_object_too_large"
        );
        assert!(store.get("native/oversized").await.unwrap().is_none());
        let streamed = store
            .put_stream(
                "native/stream",
                ByteStream::from(vec![0, 255]),
                ObjectMetadata::new(),
            )
            .await
            .unwrap();
        assert_eq!(streamed.content_length, 2);
        assert_eq!(
            store.get("native/stream").await.unwrap().unwrap().body,
            vec![0, 255]
        );
        store.delete("native/file").await.unwrap();
        store.delete("native/stream").await.unwrap();
    }
}
