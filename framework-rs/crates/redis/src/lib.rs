#![forbid(unsafe_code)]

#[cfg(feature = "redis-client")]
pub mod client;

#[cfg(feature = "encryption")]
pub mod json;

use std::{
    collections::{BTreeMap, VecDeque},
    time::{Duration, Instant},
};

use forklaunch_core::common::{FrameworkError, FrameworkResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedisTtlCacheConfig {
    pub default_ttl_milliseconds: u64,
    pub url: String,
}

impl RedisTtlCacheConfig {
    #[must_use]
    pub fn new(url: impl Into<String>, default_ttl: Duration) -> Self {
        let millis = default_ttl.as_millis();
        let default_ttl_milliseconds = u64::try_from(millis).unwrap_or(u64::MAX);

        Self {
            default_ttl_milliseconds,
            url: url.into(),
        }
    }

    #[must_use]
    pub fn default_ttl(&self) -> Duration {
        Duration::from_millis(self.default_ttl_milliseconds)
    }

    /// # Errors
    ///
    /// Returns an error when the Redis URL or default TTL is empty.
    pub fn validate(&self) -> FrameworkResult<()> {
        if self.url.trim().is_empty() {
            return Err(FrameworkError::new(
                "redis_config_invalid",
                "redis url is required",
            ));
        }

        if self.default_ttl_milliseconds == 0 {
            return Err(FrameworkError::new(
                "redis_config_invalid",
                "default ttl must be greater than zero",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheRecord {
    pub key: String,
    pub value: Vec<u8>,
    pub expires_at: Instant,
}

impl CacheRecord {
    #[must_use]
    pub fn is_expired(&self, now: Instant) -> bool {
        self.expires_at <= now
    }
}

pub trait TtlCache {
    /// # Errors
    ///
    /// Returns an error when the key is invalid or the backing cache rejects the write.
    fn set(
        &mut self,
        key: impl Into<String>,
        value: Vec<u8>,
        ttl: Option<Duration>,
    ) -> FrameworkResult<()>;

    /// # Errors
    ///
    /// Returns an error when the key is invalid or the backing cache rejects the read.
    fn get(&mut self, key: &str) -> FrameworkResult<Option<Vec<u8>>>;

    /// # Errors
    ///
    /// Returns an error when the key is invalid or the backing cache rejects the delete.
    fn delete(&mut self, key: &str) -> FrameworkResult<bool>;

    /// Returns whether a live (non-expired) value exists for the key.
    ///
    /// # Errors
    ///
    /// Returns an error when the key is invalid or the backing cache rejects the read.
    fn peek(&mut self, key: &str) -> FrameworkResult<bool> {
        Ok(self.get(key)?.is_some())
    }

    /// Checks existence in input order without decoding stored values.
    ///
    /// # Errors
    /// Returns the first backend failure. Implementations may batch the checks.
    fn peek_batch(&mut self, keys: &[String]) -> FrameworkResult<Vec<bool>> {
        keys.iter().map(|key| self.peek(key)).collect()
    }

    /// Writes several entries under a shared TTL.
    ///
    /// # Errors
    ///
    /// Returns an error on the first entry the backing cache rejects.
    fn put_batch(
        &mut self,
        entries: Vec<(String, Vec<u8>)>,
        ttl: Option<Duration>,
    ) -> FrameworkResult<()> {
        for (key, value) in entries {
            self.set(key, value, ttl)?;
        }
        Ok(())
    }

    /// Reads several keys, preserving order and returning `None` for misses.
    ///
    /// # Errors
    ///
    /// Returns an error on the first key the backing cache rejects.
    fn get_batch(&mut self, keys: &[String]) -> FrameworkResult<Vec<Option<Vec<u8>>>> {
        keys.iter().map(|key| self.get(key)).collect()
    }

    /// Deletes several keys, returning how many existed.
    ///
    /// # Errors
    ///
    /// Returns an error on the first key the backing cache rejects.
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

#[derive(Debug)]
pub struct InMemoryRedisTtlCache {
    config: RedisTtlCacheConfig,
    records: BTreeMap<String, CacheRecord>,
}

impl InMemoryRedisTtlCache {
    /// # Errors
    ///
    /// Returns an error when the cache configuration is invalid.
    pub fn new(config: RedisTtlCacheConfig) -> FrameworkResult<Self> {
        config.validate()?;
        Ok(Self {
            config,
            records: BTreeMap::new(),
        })
    }

    #[must_use]
    pub fn config(&self) -> &RedisTtlCacheConfig {
        &self.config
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn purge_expired(&mut self, now: Instant) -> usize {
        let before = self.records.len();
        self.records.retain(|_, record| !record.is_expired(now));
        before - self.records.len()
    }

    /// Lists live keys that start with `prefix`, in sorted order.
    #[must_use]
    pub fn list_keys_by_prefix(&self, prefix: &str) -> Vec<String> {
        let now = Instant::now();
        self.records
            .iter()
            .filter(|(key, record)| key.starts_with(prefix) && !record.is_expired(now))
            .map(|(key, _)| key.clone())
            .collect()
    }
}

/// An ordered, in-memory queue mirroring the Redis list-backed queue API.
#[derive(Debug, Default)]
pub struct InMemoryRedisQueue {
    items: VecDeque<Vec<u8>>,
}

impl InMemoryRedisQueue {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enqueue(&mut self, item: Vec<u8>) {
        self.items.push_back(item);
    }

    pub fn dequeue(&mut self) -> Option<Vec<u8>> {
        self.items.pop_front()
    }

    #[must_use]
    pub fn peek(&self) -> Option<&[u8]> {
        self.items.front().map(Vec::as_slice)
    }

    /// Enqueues a batch of items, preserving order.
    pub fn enqueue_batch(&mut self, items: impl IntoIterator<Item = Vec<u8>>) {
        self.items.extend(items);
    }

    /// Dequeues up to `max` items from the front of the queue.
    pub fn dequeue_batch(&mut self, max: usize) -> Vec<Vec<u8>> {
        let take = max.min(self.items.len());
        self.items.drain(..take).collect()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// A compliance hook applied to values before they are stored and after they
/// are read, enabling encryption-at-rest without coupling the cache to a cipher.
pub trait CacheCodec {
    /// Transforms a plaintext value into its stored representation.
    fn encode(&self, plaintext: &[u8]) -> Vec<u8>;

    /// Recovers the plaintext value from its stored representation.
    ///
    /// # Errors
    ///
    /// Returns an error when the stored bytes cannot be decoded.
    fn decode(&self, stored: &[u8]) -> FrameworkResult<Vec<u8>>;
}

/// A no-op codec that stores values verbatim.
#[derive(Debug, Clone, Copy, Default)]
pub struct IdentityCacheCodec;

impl CacheCodec for IdentityCacheCodec {
    fn encode(&self, plaintext: &[u8]) -> Vec<u8> {
        plaintext.to_vec()
    }

    fn decode(&self, stored: &[u8]) -> FrameworkResult<Vec<u8>> {
        Ok(stored.to_vec())
    }
}

/// A reversible XOR codec used to demonstrate the encryption hook end-to-end.
///
/// This is a placeholder transform, not a secure cipher; production deployments
/// should supply an AEAD-backed [`CacheCodec`].
#[derive(Debug, Clone)]
pub struct XorCacheCodec {
    key: Vec<u8>,
}

impl XorCacheCodec {
    /// Creates a codec from a non-empty key.
    ///
    /// # Errors
    ///
    /// Returns an error when the key is empty.
    pub fn new(key: impl Into<Vec<u8>>) -> FrameworkResult<Self> {
        let key = key.into();
        if key.is_empty() {
            return Err(FrameworkError::new(
                "redis_codec_invalid_key",
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

impl CacheCodec for XorCacheCodec {
    fn encode(&self, plaintext: &[u8]) -> Vec<u8> {
        self.transform(plaintext)
    }

    fn decode(&self, stored: &[u8]) -> FrameworkResult<Vec<u8>> {
        Ok(self.transform(stored))
    }
}

/// Writes a value through a [`CacheCodec`], applying the compliance hook.
///
/// # Errors
///
/// Returns an error when the backing cache rejects the write.
pub fn put_encrypted<C: TtlCache, K: CacheCodec>(
    cache: &mut C,
    codec: &K,
    key: impl Into<String>,
    value: &[u8],
    ttl: Option<Duration>,
) -> FrameworkResult<()> {
    cache.set(key, codec.encode(value), ttl)
}

/// Reads a value through a [`CacheCodec`], reversing the compliance hook.
///
/// # Errors
///
/// Returns an error when the read fails or the stored bytes cannot be decoded.
pub fn get_decrypted<C: TtlCache, K: CacheCodec>(
    cache: &mut C,
    codec: &K,
    key: &str,
) -> FrameworkResult<Option<Vec<u8>>> {
    match cache.get(key)? {
        Some(stored) => Ok(Some(codec.decode(&stored)?)),
        None => Ok(None),
    }
}

impl TtlCache for InMemoryRedisTtlCache {
    fn set(
        &mut self,
        key: impl Into<String>,
        value: Vec<u8>,
        ttl: Option<Duration>,
    ) -> FrameworkResult<()> {
        let key = normalize_key(key)?;
        let ttl = ttl.unwrap_or_else(|| self.config.default_ttl());
        if ttl.is_zero() {
            return Err(FrameworkError::new(
                "redis_cache_invalid_ttl",
                "cache ttl must be greater than zero",
            ));
        }

        self.records.insert(
            key.clone(),
            CacheRecord {
                key,
                value,
                expires_at: Instant::now() + ttl,
            },
        );
        Ok(())
    }

    fn get(&mut self, key: &str) -> FrameworkResult<Option<Vec<u8>>> {
        let key = normalize_key(key)?;
        if self
            .records
            .get(&key)
            .is_some_and(|record| record.is_expired(Instant::now()))
        {
            self.records.remove(&key);
            return Ok(None);
        }

        Ok(self.records.get(&key).map(|record| record.value.clone()))
    }

    fn delete(&mut self, key: &str) -> FrameworkResult<bool> {
        let key = normalize_key(key)?;
        Ok(self.records.remove(&key).is_some())
    }
}

fn normalize_key(key: impl Into<String>) -> FrameworkResult<String> {
    let key = key.into();
    if key.trim().is_empty() {
        return Err(FrameworkError::new(
            "redis_cache_invalid_key",
            "cache key is required",
        ));
    }

    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_stores_reads_and_deletes_values() {
        let config = RedisTtlCacheConfig::new("redis://localhost:6379", Duration::from_secs(60));
        let mut cache = InMemoryRedisTtlCache::new(config).expect("config should be valid");

        cache
            .set("session:1", b"abc".to_vec(), None)
            .expect("write should succeed");

        assert_eq!(
            cache.get("session:1").expect("read should succeed"),
            Some(b"abc".to_vec())
        );
        assert!(cache.delete("session:1").expect("delete should succeed"));
        assert_eq!(cache.get("session:1").expect("read should succeed"), None);
    }

    #[test]
    fn cache_purges_expired_records() {
        let config = RedisTtlCacheConfig::new("redis://localhost:6379", Duration::from_secs(60));
        let mut cache = InMemoryRedisTtlCache::new(config).expect("config should be valid");

        cache
            .set("session:1", b"abc".to_vec(), Some(Duration::from_millis(1)))
            .expect("write should succeed");

        assert_eq!(
            cache.purge_expired(Instant::now() + Duration::from_secs(1)),
            1
        );
        assert!(cache.is_empty());
    }

    fn cache() -> InMemoryRedisTtlCache {
        let config = RedisTtlCacheConfig::new("redis://localhost:6379", Duration::from_secs(60));
        InMemoryRedisTtlCache::new(config).expect("config should be valid")
    }

    #[test]
    fn batch_operations_and_prefix_listing() {
        let mut cache = cache();
        cache
            .put_batch(
                vec![
                    ("user:1".to_owned(), b"a".to_vec()),
                    ("user:2".to_owned(), b"b".to_vec()),
                    ("session:1".to_owned(), b"c".to_vec()),
                ],
                None,
            )
            .expect("batch write should succeed");

        let read = cache
            .get_batch(&["user:1".to_owned(), "missing".to_owned()])
            .expect("batch read should succeed");
        assert_eq!(read, vec![Some(b"a".to_vec()), None]);

        assert!(cache.peek("user:2").expect("peek should succeed"));

        let mut keys = cache.list_keys_by_prefix("user:");
        keys.sort();
        assert_eq!(keys, vec!["user:1".to_owned(), "user:2".to_owned()]);

        let deleted = cache
            .delete_batch(&["user:1".to_owned(), "user:2".to_owned()])
            .expect("batch delete should succeed");
        assert_eq!(deleted, 2);
    }

    #[test]
    fn queue_supports_fifo_and_batch_operations() {
        let mut queue = InMemoryRedisQueue::new();
        queue.enqueue(b"first".to_vec());
        queue.enqueue_batch([b"second".to_vec(), b"third".to_vec()]);

        assert_eq!(queue.peek(), Some(b"first".as_slice()));
        assert_eq!(queue.dequeue(), Some(b"first".to_vec()));

        let drained = queue.dequeue_batch(5);
        assert_eq!(drained, vec![b"second".to_vec(), b"third".to_vec()]);
        assert!(queue.is_empty());
    }

    #[test]
    fn encryption_hook_round_trips_and_stores_ciphertext() {
        let mut cache = cache();
        let codec = XorCacheCodec::new(b"secret-key".to_vec()).expect("codec key should be valid");

        put_encrypted(&mut cache, &codec, "token:1", b"plaintext", None)
            .expect("encrypted write should succeed");

        // stored value is not the plaintext
        let stored = cache.get("token:1").expect("read should succeed").unwrap();
        assert_ne!(stored, b"plaintext".to_vec());

        // decoding recovers the plaintext
        let recovered =
            get_decrypted(&mut cache, &codec, "token:1").expect("decrypted read should succeed");
        assert_eq!(recovered, Some(b"plaintext".to_vec()));
    }
}
