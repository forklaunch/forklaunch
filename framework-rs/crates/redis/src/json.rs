//! Typed JSON records; encryption errors propagate before the backing write.
use crate::TtlCache;
use forklaunch_core::{
    common::FrameworkResult,
    persistence::{TenantContext, json::JsonRecordCodec},
};
use serde::{Serialize, de::DeserializeOwned};
use std::time::Duration;

pub struct JsonCache<C> {
    cache: C,
    codec: JsonRecordCodec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonCacheRecord<T> {
    pub key: String,
    pub value: T,
    /// Millisecond-precision expiry; `None` denotes an externally written persistent key.
    pub remaining_ttl: Option<Duration>,
}

pub struct JsonCacheWrite<T> {
    pub key: String,
    pub value: T,
    /// `None` selects the cache's configured default TTL.
    pub ttl: Option<Duration>,
}

#[cfg(feature = "redis-client")]
impl JsonCache<crate::client::RedisTtlCache> {
    /// Returns sorted unique keys for a literal prefix using SCAN.
    /// # Errors
    /// Propagates backend failures.
    pub fn list_keys_by_prefix(&mut self, prefix: &str) -> FrameworkResult<Vec<String>> {
        self.cache.list_keys_by_prefix(prefix)
    }

    /// Lists sorted unique keys matching a Redis glob pattern, without decoding.
    /// Append `*` to reproduce TypeScript's `listKeys(prefix)` pattern semantics.
    /// # Errors
    /// Propagates backend failures.
    pub fn list_keys_matching(&mut self, pattern: &str) -> FrameworkResult<Vec<String>> {
        self.cache.list_keys_matching(pattern)
    }

    /// Scans a literal prefix, then checks existence without reading/decrypting
    /// values. Retains keys with a false result if they disappeared after the scan.
    /// # Errors
    /// Propagates backend failures.
    pub fn peek_records_by_prefix(&mut self, prefix: &str) -> FrameworkResult<Vec<(String, bool)>> {
        let keys = self.list_keys_by_prefix(prefix)?;
        let present = self.cache.peek_batch(&keys)?;
        Ok(keys.into_iter().zip(present).collect())
    }

    /// Serializes every value and validates every key/TTL before sending one transaction.
    /// # Errors
    /// Propagates JSON, encryption, validation and backend failures.
    pub fn put_records<T: Serialize>(
        &mut self,
        records: Vec<JsonCacheWrite<T>>,
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<()> {
        let records = records
            .into_iter()
            .map(|record| {
                Ok(crate::client::RedisWriteRecord {
                    key: record.key,
                    value: self.codec.encode(&record.value, tenant)?,
                    ttl: record.ttl,
                })
            })
            .collect::<FrameworkResult<Vec<_>>>()?;
        self.cache.put_batch_with_ttls(records)
    }

    /// Reads all value/expiry pairs in one Redis transaction, keeping order and misses.
    /// # Errors
    /// Propagates validation, backend, JSON and authentication failures.
    pub fn read_records<T: DeserializeOwned>(
        &mut self,
        keys: &[String],
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<Vec<Option<JsonCacheRecord<T>>>> {
        keys.iter()
            .zip(self.cache.get_batch_with_ttl(keys)?)
            .map(|(key, record)| {
                record
                    .map(|record| {
                        Ok(JsonCacheRecord {
                            key: key.clone(),
                            value: self.codec.decode(&record.value, tenant)?,
                            remaining_ttl: record.remaining_ttl,
                        })
                    })
                    .transpose()
            })
            .collect()
    }

    /// Uses a literal prefix (SCAN), then reads matched records with expiry metadata.
    /// Concurrent additions/deletions may change which keys the scan observes.
    /// # Errors
    /// Propagates backend, JSON and authentication failures.
    pub fn read_records_by_prefix<T: DeserializeOwned>(
        &mut self,
        prefix: &str,
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<Vec<JsonCacheRecord<T>>> {
        let keys = self.cache.list_keys_by_prefix(prefix)?;
        Ok(self
            .read_records(&keys, tenant)?
            .into_iter()
            .flatten()
            .collect())
    }
    /// Reads JSON with its Redis expiry without rounding seconds or replacing misses.
    /// # Errors
    /// Propagates backend, JSON and authentication failures.
    pub fn read_record_with_ttl<T: DeserializeOwned>(
        &mut self,
        key: &str,
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<Option<JsonCacheRecord<T>>> {
        self.cache
            .get_with_ttl(key)?
            .map(|record| {
                Ok(JsonCacheRecord {
                    key: key.to_owned(),
                    value: self.codec.decode(&record.value, tenant)?,
                    remaining_ttl: record.remaining_ttl,
                })
            })
            .transpose()
    }
}

impl<C: TtlCache> JsonCache<C> {
    #[must_use]
    pub fn new(cache: C, codec: JsonRecordCodec) -> Self {
        Self { cache, codec }
    }

    #[must_use]
    pub fn into_inner(self) -> C {
        self.cache
    }

    #[must_use]
    pub const fn backend(&self) -> &C {
        &self.cache
    }

    /// Access the underlying backend. Direct writes bypass this JSON codec.
    pub fn backend_mut(&mut self) -> &mut C {
        &mut self.cache
    }

    /// Checks existence without needing a tenant or decoding/decrypting the value.
    /// # Errors
    /// Propagates backend failures.
    pub fn peek_record(&mut self, key: &str) -> FrameworkResult<bool> {
        self.cache.peek(key)
    }

    /// Preserves order, duplicates and misses without decoding/decrypting values.
    /// # Errors
    /// Propagates backend failures.
    pub fn peek_batch_records(&mut self, keys: &[String]) -> FrameworkResult<Vec<bool>> {
        self.cache.peek_batch(keys)
    }

    /// # Errors
    /// Propagates serialization, encryption, invalid TTL and backend failures.
    pub fn put_record<T: Serialize + ?Sized>(
        &mut self,
        key: &str,
        value: &T,
        ttl: Option<Duration>,
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<()> {
        self.cache.set(key, self.codec.encode(value, tenant)?, ttl)
    }

    /// Missing keys are `None`; authentication and JSON failures remain errors.
    /// # Errors
    /// Propagates backend, JSON and decryption failures.
    pub fn read_record<T: DeserializeOwned>(
        &mut self,
        key: &str,
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<Option<T>> {
        self.cache
            .get(key)?
            .map(|bytes| self.codec.decode(&bytes, tenant))
            .transpose()
    }

    /// Serializes every entry before writing any of them.
    /// # Errors
    /// Propagates serialization, encryption and backend failures.
    pub fn put_batch_records<T: Serialize>(
        &mut self,
        entries: Vec<(String, T)>,
        ttl: Option<Duration>,
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<()> {
        let entries = entries
            .into_iter()
            .map(|(key, value)| Ok((key, self.codec.encode(&value, tenant)?)))
            .collect::<FrameworkResult<Vec<_>>>()?;
        self.cache.put_batch(entries, ttl)
    }

    /// Preserves input order and missing keys, including false, zero and null values.
    /// # Errors
    /// Propagates backend, JSON and authentication failures.
    pub fn read_batch_records<T: DeserializeOwned>(
        &mut self,
        keys: &[String],
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<Vec<Option<T>>> {
        self.cache
            .get_batch(keys)?
            .into_iter()
            .map(|bytes| {
                bytes
                    .map(|bytes| self.codec.decode(&bytes, tenant))
                    .transpose()
            })
            .collect()
    }

    /// # Errors
    /// Propagates backend failures.
    pub fn delete_record(&mut self, key: &str) -> FrameworkResult<bool> {
        self.cache.delete(key)
    }

    /// Deletes keys without decoding their values, returning the number removed.
    /// # Errors
    /// Propagates backend failures.
    pub fn delete_batch_records(&mut self, keys: &[String]) -> FrameworkResult<usize> {
        self.cache.delete_batch(keys)
    }
}

/// Typed list-backed queue using the same JSON/encryption codec as cache records.
#[cfg(feature = "redis-client")]
pub struct JsonQueue {
    queue: crate::client::RedisQueue,
    codec: JsonRecordCodec,
}

#[cfg(feature = "redis-client")]
impl JsonQueue {
    #[must_use]
    pub fn new(queue: crate::client::RedisQueue, codec: JsonRecordCodec) -> Self {
        Self { queue, codec }
    }

    #[must_use]
    pub fn into_inner(self) -> crate::client::RedisQueue {
        self.queue
    }

    /// # Errors
    /// Propagates serialization, encryption and Redis failures.
    pub fn enqueue_record<T: Serialize + ?Sized>(
        &mut self,
        value: &T,
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<()> {
        self.queue.enqueue(&self.codec.encode(value, tenant)?)
    }

    /// Serializes all values before enqueueing the batch.
    /// # Errors
    /// Propagates serialization, encryption and Redis failures.
    pub fn enqueue_batch_records<T: Serialize>(
        &mut self,
        values: &[T],
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<()> {
        let values = values
            .iter()
            .map(|value| self.codec.encode(value, tenant))
            .collect::<FrameworkResult<Vec<_>>>()?;
        self.queue.enqueue_batch(&values)
    }

    /// Removal precedes decoding, as with TypeScript RPOP; errors do not requeue data.
    /// Use peek to validate context before consumption when needed.
    /// # Errors
    /// Propagates Redis, JSON and authentication failures.
    pub fn dequeue_record<T: DeserializeOwned>(
        &mut self,
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<Option<T>> {
        self.queue
            .dequeue()?
            .map(|value| self.codec.decode(&value, tenant))
            .transpose()
    }

    /// Preserves FIFO order and false/zero/null values. Empty queues yield an empty batch.
    /// Removal precedes decoding; decoding failures do not requeue removed items.
    /// # Errors
    /// Propagates Redis, JSON and authentication failures.
    pub fn dequeue_batch_records<T: DeserializeOwned>(
        &mut self,
        max: usize,
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<Vec<T>> {
        self.queue
            .dequeue_batch(max)?
            .iter()
            .map(|value| self.codec.decode(value, tenant))
            .collect()
    }

    /// Matches TypeScript bulk queue reads by removing falsy decoded values.
    /// The count limits removed items before filtering, not the returned length.
    /// Zero removes nothing. Authentication failures are not filtered out.
    ///
    /// # Errors
    /// Propagates Redis, JSON, type and authentication errors. Removed items are
    /// not requeued when decoding fails, matching the ordinary dequeue methods.
    pub fn dequeue_batch_records_typescript<T: DeserializeOwned>(
        &mut self,
        max: usize,
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<Vec<T>> {
        let values = self.queue.dequeue_batch(max)?;
        self.decode_typescript_batch(&values, tenant)
    }

    /// # Errors
    /// Propagates Redis, JSON and authentication failures without removing the item.
    pub fn peek_record<T: DeserializeOwned>(
        &mut self,
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<Option<T>> {
        self.queue
            .peek()?
            .map(|value| self.codec.decode(&value, tenant))
            .transpose()
    }

    /// # Errors
    /// Propagates Redis, JSON and authentication failures without removing items.
    pub fn peek_records<T: DeserializeOwned>(
        &mut self,
        max: usize,
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<Vec<T>> {
        self.queue
            .peek_batch(max)?
            .iter()
            .map(|value| self.codec.decode(value, tenant))
            .collect()
    }

    /// Matches TypeScript bulk peeks, including truthiness filtering and a zero
    /// count reading the entire queue. Empty arrays/objects are retained.
    /// Use `peek_records` for native zero-count and falsy-preserving behavior.
    ///
    /// # Errors
    /// Propagates Redis, JSON, type and authentication errors without removing items.
    pub fn peek_records_typescript<T: DeserializeOwned>(
        &mut self,
        max: usize,
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<Vec<T>> {
        let values = if max == 0 {
            self.queue.peek_all()?
        } else {
            self.queue.peek_batch(max)?
        };
        self.decode_typescript_batch(&values, tenant)
    }

    fn decode_typescript_batch<T: DeserializeOwned>(
        &self,
        values: &[Vec<u8>],
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<Vec<T>> {
        let mut output = Vec::new();
        for value in values {
            if let Some(value) = self.codec.decode_typescript_truthy(value, tenant)? {
                output.push(value);
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InMemoryRedisTtlCache, RedisTtlCacheConfig};
    use forklaunch_core::persistence::encryption::FieldEncryptor;
    use serde::Serializer;

    struct Fallible(bool);
    impl Serialize for Fallible {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            if self.0 {
                serializer.serialize_bool(false)
            } else {
                Err(serde::ser::Error::custom("secret must never escape"))
            }
        }
    }

    #[test]
    fn serialization_failure_does_not_commit_a_partial_batch_or_expose_secrets() {
        let cache = InMemoryRedisTtlCache::new(RedisTtlCacheConfig::new(
            "redis://fixture",
            Duration::from_secs(60),
        ))
        .unwrap();
        let mut cache = JsonCache::new(
            cache,
            JsonRecordCodec::new(FieldEncryptor::new("fixture-key").unwrap()),
        );
        let tenant = TenantContext::new("tenant-a").unwrap();
        let error = cache
            .put_batch_records(
                vec![
                    ("first".into(), Fallible(true)),
                    ("second".into(), Fallible(false)),
                ],
                None,
                Some(&tenant),
            )
            .unwrap_err();
        assert_eq!(error.code(), "record_json_invalid");
        assert!(!error.to_string().contains("secret"));
        assert!(
            cache
                .read_record::<bool>("first", Some(&tenant))
                .unwrap()
                .is_none()
        );
    }
}
