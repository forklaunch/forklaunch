#![forbid(unsafe_code)]

use std::{collections::BTreeMap, future::Future};

#[cfg(feature = "aws-sdk")]
pub mod aws;

#[cfg(feature = "encryption")]
pub mod json;

use forklaunch_core::common::{FrameworkError, FrameworkResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct S3ObjectStoreConfig {
    pub bucket: String,
    pub region: Option<String>,
}

impl S3ObjectStoreConfig {
    #[must_use]
    pub fn new(bucket: impl Into<String>) -> Self {
        Self {
            bucket: bucket.into(),
            region: None,
        }
    }

    #[must_use]
    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region = Some(region.into());
        self
    }

    /// # Errors
    ///
    /// Returns an error when the bucket name is empty.
    pub fn validate(&self) -> FrameworkResult<()> {
        if self.bucket.trim().is_empty() {
            return Err(FrameworkError::new(
                "s3_config_invalid",
                "bucket name is required",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectMetadata {
    pub content_type: Option<String>,
    pub tags: BTreeMap<String, String>,
}

impl ObjectMetadata {
    #[must_use]
    pub fn new() -> Self {
        Self {
            content_type: None,
            tags: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    #[must_use]
    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }
}

impl Default for ObjectMetadata {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredObject {
    pub key: String,
    pub body: Vec<u8>,
    pub metadata: ObjectMetadata,
}

pub trait ObjectStore {
    /// # Errors
    ///
    /// Returns an error when the key is invalid or the backing object store rejects the write.
    fn put(
        &mut self,
        key: impl Into<String>,
        body: Vec<u8>,
        metadata: ObjectMetadata,
    ) -> FrameworkResult<StoredObject>;

    /// # Errors
    ///
    /// Returns an error when the key is invalid or the backing object store rejects the read.
    fn get(&self, key: &str) -> FrameworkResult<Option<StoredObject>>;

    /// # Errors
    ///
    /// Returns an error when the key is invalid or the backing object store rejects the delete.
    fn delete(&mut self, key: &str) -> FrameworkResult<bool>;

    /// # Errors
    ///
    /// Returns an error when the prefix is invalid or the backing object store rejects the list.
    fn list(&self, prefix: &str) -> FrameworkResult<Vec<StoredObject>>;

    /// Writes several objects, returning the stored representations in order.
    ///
    /// # Errors
    ///
    /// Returns an error on the first object the backing store rejects.
    fn put_batch(
        &mut self,
        objects: Vec<(String, Vec<u8>, ObjectMetadata)>,
    ) -> FrameworkResult<Vec<StoredObject>> {
        objects
            .into_iter()
            .map(|(key, body, metadata)| self.put(key, body, metadata))
            .collect()
    }

    /// Reads several keys, preserving order and returning `None` for misses.
    ///
    /// # Errors
    ///
    /// Returns an error on the first key the backing store rejects.
    fn get_batch(&self, keys: &[String]) -> FrameworkResult<Vec<Option<StoredObject>>> {
        keys.iter().map(|key| self.get(key)).collect()
    }

    /// Deletes several keys, returning how many existed.
    ///
    /// # Errors
    ///
    /// Returns an error on the first key the backing store rejects.
    fn delete_batch(&mut self, keys: &[String]) -> FrameworkResult<usize> {
        let mut deleted = 0;
        for key in keys {
            if self.delete(key)? {
                deleted += 1;
            }
        }
        Ok(deleted)
    }
}

/// Async storage boundary for network-backed object stores.
pub trait AsyncObjectStore: Send + Sync {
    /// # Errors
    /// Returns an error for invalid input or a failed backing-store write.
    fn put(
        &self,
        key: &str,
        body: Vec<u8>,
        metadata: ObjectMetadata,
    ) -> impl Future<Output = FrameworkResult<StoredObject>> + Send;
    /// # Errors
    /// Returns an error for invalid input or a failed backing-store read.
    fn get(&self, key: &str) -> impl Future<Output = FrameworkResult<Option<StoredObject>>> + Send;
    /// # Errors
    /// Returns an error for invalid input or a failed backing-store delete.
    fn delete(&self, key: &str) -> impl Future<Output = FrameworkResult<bool>> + Send;
    /// # Errors
    /// Returns an error for invalid input, exceeded limits, or a failed listing.
    fn list(&self, prefix: &str)
    -> impl Future<Output = FrameworkResult<Vec<StoredObject>>> + Send;
    /// # Errors
    /// Returns an error on the first failed write; earlier writes remain committed.
    fn put_batch(
        &self,
        objects: Vec<(String, Vec<u8>, ObjectMetadata)>,
    ) -> impl Future<Output = FrameworkResult<Vec<StoredObject>>> + Send {
        async move {
            let mut stored = Vec::with_capacity(objects.len());
            for (key, body, metadata) in objects {
                stored.push(self.put(&key, body, metadata).await?);
            }
            Ok(stored)
        }
    }
    /// # Errors
    /// Returns an error on the first failed read, preserving key order and misses.
    fn get_batch(
        &self,
        keys: &[String],
    ) -> impl Future<Output = FrameworkResult<Vec<Option<StoredObject>>>> + Send {
        async move {
            let mut objects = Vec::with_capacity(keys.len());
            for key in keys {
                objects.push(self.get(key).await?);
            }
            Ok(objects)
        }
    }
    /// # Errors
    /// Returns an error on the first failed delete; earlier deletions remain committed.
    fn delete_batch(&self, keys: &[String]) -> impl Future<Output = FrameworkResult<usize>> + Send {
        async move {
            let mut count = 0;
            for key in keys {
                count += usize::from(self.delete(key).await?);
            }
            Ok(count)
        }
    }
}

#[derive(Debug)]
pub struct InMemoryS3ObjectStore {
    config: S3ObjectStoreConfig,
    objects: BTreeMap<String, StoredObject>,
}

impl InMemoryS3ObjectStore {
    /// # Errors
    ///
    /// Returns an error when the object store configuration is invalid.
    pub fn new(config: S3ObjectStoreConfig) -> FrameworkResult<Self> {
        config.validate()?;
        Ok(Self {
            config,
            objects: BTreeMap::new(),
        })
    }

    #[must_use]
    pub fn config(&self) -> &S3ObjectStoreConfig {
        &self.config
    }

    /// Ensures the configured bucket exists and is ready for use.
    ///
    /// For the in-memory store this validates configuration; real backends
    /// create the bucket if it is absent.
    ///
    /// # Errors
    ///
    /// Returns an error when the configuration is invalid.
    pub fn init_bucket(&self) -> FrameworkResult<()> {
        self.config.validate()
    }
}

/// Buffers chunks for a streaming upload, then commits them as one object.
#[derive(Debug)]
pub struct StreamingUpload {
    key: String,
    metadata: ObjectMetadata,
    buffer: Vec<u8>,
}

impl StreamingUpload {
    #[must_use]
    pub fn new(key: impl Into<String>, metadata: ObjectMetadata) -> Self {
        Self {
            key: key.into(),
            metadata,
            buffer: Vec::new(),
        }
    }

    /// Appends a chunk to the pending upload.
    pub fn write_chunk(&mut self, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);
    }

    /// Commits the buffered chunks to the store as a single object.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store rejects the write.
    pub fn finish<S: ObjectStore>(self, store: &mut S) -> FrameworkResult<StoredObject> {
        store.put(self.key, self.buffer, self.metadata)
    }
}

/// Reads an object back as a sequence of fixed-size chunks (streaming download).
///
/// # Errors
///
/// Returns an error when the backing store rejects the read.
pub fn download_chunks<S: ObjectStore>(
    store: &S,
    key: &str,
    chunk_size: usize,
) -> FrameworkResult<Option<Vec<Vec<u8>>>> {
    let chunk_size = chunk_size.max(1);
    Ok(store
        .get(key)?
        .map(|object| object.body.chunks(chunk_size).map(<[u8]>::to_vec).collect()))
}

/// A compliance hook applied to object bodies before storage and after reads.
pub trait ObjectCodec {
    /// Transforms a plaintext body into its stored representation.
    fn encode(&self, plaintext: &[u8]) -> Vec<u8>;

    /// Recovers the plaintext body from its stored representation.
    ///
    /// # Errors
    ///
    /// Returns an error when the stored bytes cannot be decoded.
    fn decode(&self, stored: &[u8]) -> FrameworkResult<Vec<u8>>;
}

/// A no-op codec that stores object bodies verbatim.
#[derive(Debug, Clone, Copy, Default)]
pub struct IdentityObjectCodec;

impl ObjectCodec for IdentityObjectCodec {
    fn encode(&self, plaintext: &[u8]) -> Vec<u8> {
        plaintext.to_vec()
    }

    fn decode(&self, stored: &[u8]) -> FrameworkResult<Vec<u8>> {
        Ok(stored.to_vec())
    }
}

/// A reversible XOR codec demonstrating the encryption hook end-to-end.
///
/// This is a placeholder transform, not a secure cipher; production deployments
/// should supply an AEAD-backed [`ObjectCodec`].
#[derive(Debug, Clone)]
pub struct XorObjectCodec {
    key: Vec<u8>,
}

impl XorObjectCodec {
    /// Creates a codec from a non-empty key.
    ///
    /// # Errors
    ///
    /// Returns an error when the key is empty.
    pub fn new(key: impl Into<Vec<u8>>) -> FrameworkResult<Self> {
        let key = key.into();
        if key.is_empty() {
            return Err(FrameworkError::new(
                "s3_codec_invalid_key",
                "codec key must not be empty",
            ));
        }
        Ok(Self { key })
    }

    fn transform(&self, bytes: &[u8]) -> Vec<u8> {
        bytes
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ self.key[index % self.key.len()])
            .collect()
    }
}

impl ObjectCodec for XorObjectCodec {
    fn encode(&self, plaintext: &[u8]) -> Vec<u8> {
        self.transform(plaintext)
    }

    fn decode(&self, stored: &[u8]) -> FrameworkResult<Vec<u8>> {
        Ok(self.transform(stored))
    }
}

/// Writes an object body through an [`ObjectCodec`], applying the compliance hook.
///
/// # Errors
///
/// Returns an error when the backing store rejects the write.
pub fn put_encrypted<S: ObjectStore, C: ObjectCodec>(
    store: &mut S,
    codec: &C,
    key: impl Into<String>,
    body: &[u8],
    metadata: ObjectMetadata,
) -> FrameworkResult<StoredObject> {
    store.put(key, codec.encode(body), metadata)
}

/// Reads an object body through an [`ObjectCodec`], reversing the compliance hook.
///
/// # Errors
///
/// Returns an error when the read fails or the stored bytes cannot be decoded.
pub fn get_decrypted<S: ObjectStore, C: ObjectCodec>(
    store: &S,
    codec: &C,
    key: &str,
) -> FrameworkResult<Option<Vec<u8>>> {
    match store.get(key)? {
        Some(object) => Ok(Some(codec.decode(&object.body)?)),
        None => Ok(None),
    }
}

/// Writes an object through a compliance codec using an async backing store.
/// # Errors
/// Returns an error if the backing store rejects the encoded object.
pub async fn put_encrypted_async<S: AsyncObjectStore, C: ObjectCodec>(
    store: &S,
    codec: &C,
    key: &str,
    body: &[u8],
    metadata: ObjectMetadata,
) -> FrameworkResult<StoredObject> {
    store.put(key, codec.encode(body), metadata).await
}

/// Reads and decodes an object using an async backing store.
/// # Errors
/// Returns an error if storage access or codec verification fails.
pub async fn get_decrypted_async<S: AsyncObjectStore, C: ObjectCodec>(
    store: &S,
    codec: &C,
    key: &str,
) -> FrameworkResult<Option<Vec<u8>>> {
    store
        .get(key)
        .await?
        .map(|object| codec.decode(&object.body))
        .transpose()
}

impl ObjectStore for InMemoryS3ObjectStore {
    fn put(
        &mut self,
        key: impl Into<String>,
        body: Vec<u8>,
        metadata: ObjectMetadata,
    ) -> FrameworkResult<StoredObject> {
        let key = normalize_key(key)?;
        let object = StoredObject {
            key: key.clone(),
            body,
            metadata,
        };
        self.objects.insert(key, object.clone());
        Ok(object)
    }

    fn get(&self, key: &str) -> FrameworkResult<Option<StoredObject>> {
        let key = normalize_key(key)?;
        Ok(self.objects.get(&key).cloned())
    }

    fn delete(&mut self, key: &str) -> FrameworkResult<bool> {
        let key = normalize_key(key)?;
        Ok(self.objects.remove(&key).is_some())
    }

    fn list(&self, prefix: &str) -> FrameworkResult<Vec<StoredObject>> {
        if prefix.contains("..") {
            return Err(FrameworkError::new(
                "s3_object_invalid_prefix",
                "object prefix cannot contain parent traversal",
            ));
        }

        Ok(self
            .objects
            .values()
            .filter(|object| object.key.starts_with(prefix))
            .cloned()
            .collect())
    }
}

fn normalize_key(key: impl Into<String>) -> FrameworkResult<String> {
    let key = key.into();
    if key.trim().is_empty() {
        return Err(FrameworkError::new(
            "s3_object_invalid_key",
            "object key is required",
        ));
    }

    if key.contains("..") {
        return Err(FrameworkError::new(
            "s3_object_invalid_key",
            "object key cannot contain parent traversal",
        ));
    }

    let key = key.trim_start_matches('/');
    if key.is_empty() {
        return Err(FrameworkError::new(
            "s3_object_invalid_key",
            "object key is required",
        ));
    }
    Ok(key.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_store_writes_reads_lists_and_deletes_objects() {
        let config = S3ObjectStoreConfig::new("assets").with_region("us-east-1");
        let mut store = InMemoryS3ObjectStore::new(config).expect("config should be valid");

        let metadata = ObjectMetadata::new()
            .with_content_type("application/json")
            .with_tag("service", "accounts");
        store
            .put("accounts/users.json", br"[]".to_vec(), metadata)
            .expect("put should succeed");

        let listed = store.list("accounts/").expect("list should succeed");
        assert_eq!(listed.len(), 1);
        assert_eq!(
            store
                .get("accounts/users.json")
                .expect("get should succeed")
                .as_ref()
                .map(|object| object.body.as_slice()),
            Some(br"[]".as_slice())
        );
        assert!(
            store
                .delete("accounts/users.json")
                .expect("delete should succeed")
        );
    }

    fn store() -> InMemoryS3ObjectStore {
        let config = S3ObjectStoreConfig::new("assets").with_region("us-east-1");
        InMemoryS3ObjectStore::new(config).expect("config should be valid")
    }

    #[test]
    fn batch_operations_and_bucket_init() {
        let mut store = store();
        store.init_bucket().expect("bucket init should succeed");

        store
            .put_batch(vec![
                ("a.txt".to_owned(), b"a".to_vec(), ObjectMetadata::new()),
                ("b.txt".to_owned(), b"b".to_vec(), ObjectMetadata::new()),
            ])
            .expect("batch put should succeed");

        let read = store
            .get_batch(&["a.txt".to_owned(), "missing".to_owned()])
            .expect("batch get should succeed");
        assert_eq!(
            read[0].as_ref().map(|object| object.body.as_slice()),
            Some(b"a".as_slice())
        );
        assert!(read[1].is_none());

        let deleted = store
            .delete_batch(&["a.txt".to_owned(), "b.txt".to_owned()])
            .expect("batch delete should succeed");
        assert_eq!(deleted, 2);
    }

    #[test]
    fn streaming_upload_and_download() {
        let mut store = store();
        let mut upload = StreamingUpload::new("report.csv", ObjectMetadata::new());
        upload.write_chunk(b"row1,");
        upload.write_chunk(b"row2,");
        upload.write_chunk(b"row3");
        upload.finish(&mut store).expect("upload should commit");

        let chunks = download_chunks(&store, "report.csv", 4)
            .expect("download should succeed")
            .expect("object should exist");
        let reassembled: Vec<u8> = chunks.concat();
        assert_eq!(reassembled, b"row1,row2,row3".to_vec());
        assert!(chunks.len() > 1);
    }

    #[test]
    fn encryption_hook_round_trips_and_stores_ciphertext() {
        let mut store = store();
        let codec = XorObjectCodec::new(b"object-key".to_vec()).expect("codec key should be valid");

        put_encrypted(
            &mut store,
            &codec,
            "secret.bin",
            b"top secret",
            ObjectMetadata::new(),
        )
        .expect("encrypted put should succeed");

        let stored = store
            .get("secret.bin")
            .expect("read should succeed")
            .unwrap();
        assert_ne!(stored.body, b"top secret".to_vec());

        let recovered =
            get_decrypted(&store, &codec, "secret.bin").expect("decrypted read should succeed");
        assert_eq!(recovered, Some(b"top secret".to_vec()));
    }
}
