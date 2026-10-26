//! Redis-backed binary TTL records and FIFO queues.
//!
//! This synchronous adapter implements the framework's synchronous cache trait.
//! In async handlers use a blocking executor; socket operations have bounded timeouts.
use super::{RedisTtlCacheConfig, TtlCache, normalize_key};
use forklaunch_core::common::{FrameworkError, FrameworkResult};
use redis::{Connection, FromRedisValue};
use std::{collections::BTreeSet, time::Duration};

const TIMEOUT: Duration = Duration::from_secs(5);

fn backend_error<E>(_: E) -> FrameworkError {
    // Server error strings and connection URLs can contain credentials or data.
    FrameworkError::new("redis_backend_error", "Redis operation failed")
}

fn connect(url: &str) -> FrameworkResult<Connection> {
    let client = redis::Client::open(url).map_err(backend_error)?;
    let connection = client
        .get_connection_with_timeout(TIMEOUT)
        .map_err(backend_error)?;
    connection
        .set_read_timeout(Some(TIMEOUT))
        .map_err(backend_error)?;
    connection
        .set_write_timeout(Some(TIMEOUT))
        .map_err(backend_error)?;
    Ok(connection)
}

fn ttl_millis(ttl: Duration) -> FrameworkResult<u64> {
    let millis = u64::try_from(ttl.as_millis())
        .ok()
        .filter(|millis| *millis > 0 && i64::try_from(*millis).is_ok());
    millis.ok_or_else(|| {
        FrameworkError::new(
            "redis_cache_invalid_ttl",
            "TTL must be a positive Redis millisecond value",
        )
    })
}

/// A cache using Redis expiration; values survive clients and processes.
pub struct RedisTtlCache {
    config: RedisTtlCacheConfig,
    connection: Connection,
}

/// A live value and Redis's remaining expiry, read in one transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedisRecord {
    pub value: Vec<u8>,
    /// `None` means a persistent key written by an external client.
    pub remaining_ttl: Option<Duration>,
}

pub struct RedisWriteRecord {
    pub key: String,
    pub value: Vec<u8>,
    /// `None` selects the configured default TTL.
    pub ttl: Option<Duration>,
}

fn live_record(value: Option<Vec<u8>>, ttl: i64) -> FrameworkResult<Option<RedisRecord>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if ttl == -2 {
        return Ok(None);
    }
    let remaining_ttl = if ttl == -1 {
        None
    } else {
        Some(Duration::from_millis(u64::try_from(ttl).map_err(|_| {
            FrameworkError::new("redis_backend_error", "Invalid Redis expiration response")
        })?))
    };
    Ok(Some(RedisRecord {
        value,
        remaining_ttl,
    }))
}

impl RedisTtlCache {
    /// # Errors
    /// Returns an error for invalid configuration or connection failure.
    pub fn connect(config: RedisTtlCacheConfig) -> FrameworkResult<Self> {
        config.validate()?;
        ttl_millis(config.default_ttl())?;
        let connection = connect(&config.url)?;
        Ok(Self { config, connection })
    }

    #[must_use]
    pub const fn config(&self) -> &RedisTtlCacheConfig {
        &self.config
    }

    /// Native equivalent of the provider client escape hatch. Raw commands bypass
    /// cache key/TTL validation and any outer JSON/encryption codec.
    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }

    /// Retrieves the value and millisecond expiry together using MULTI/EXEC.
    /// # Errors
    /// Rejects invalid keys and backend failures.
    pub fn get_with_ttl(&mut self, key: &str) -> FrameworkResult<Option<RedisRecord>> {
        let key = normalize_key(key)?;
        let (value, ttl): (Option<Vec<u8>>, i64) = redis::pipe()
            .atomic()
            .cmd("GET")
            .arg(&key)
            .cmd("PTTL")
            .arg(&key)
            .query(&mut self.connection)
            .map_err(backend_error)?;
        live_record(value, ttl)
    }

    /// Reads every value/expiry pair in one transaction, preserving key order and misses.
    /// # Errors
    /// Rejects invalid keys and backend failures.
    pub fn get_batch_with_ttl(
        &mut self,
        keys: &[String],
    ) -> FrameworkResult<Vec<Option<RedisRecord>>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let keys = keys
            .iter()
            .map(|key| normalize_key(key.as_str()))
            .collect::<FrameworkResult<Vec<_>>>()?;
        let mut pipeline = redis::pipe();
        pipeline.atomic();
        for key in &keys {
            pipeline.cmd("GET").arg(key).cmd("PTTL").arg(key);
        }
        let values: Vec<redis::Value> = pipeline
            .query(&mut self.connection)
            .map_err(backend_error)?;
        if values.len() != keys.len().saturating_mul(2) {
            return Err(FrameworkError::new(
                "redis_backend_error",
                "Invalid Redis batch response",
            ));
        }
        values
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| {
                live_record(
                    Option::<Vec<u8>>::from_redis_value_ref(&pair[0]).map_err(backend_error)?,
                    i64::from_redis_value_ref(&pair[1]).map_err(backend_error)?,
                )
            })
            .collect()
    }

    /// Validates all keys/TTLs before sending one transaction with per-record expirations.
    /// # Errors
    /// Rejects invalid keys, non-positive/overflowing TTLs and backend failures.
    pub fn put_batch_with_ttls(&mut self, entries: Vec<RedisWriteRecord>) -> FrameworkResult<()> {
        let entries = entries
            .into_iter()
            .map(|entry| {
                Ok((
                    normalize_key(entry.key)?,
                    entry.value,
                    ttl_millis(entry.ttl.unwrap_or_else(|| self.config.default_ttl()))?,
                ))
            })
            .collect::<FrameworkResult<Vec<_>>>()?;
        if entries.is_empty() {
            return Ok(());
        }
        let mut pipeline = redis::pipe();
        pipeline.atomic();
        for (key, value, ttl) in entries {
            pipeline
                .cmd("SET")
                .arg(key)
                .arg(value)
                .arg("PX")
                .arg(ttl)
                .ignore();
        }
        pipeline.query(&mut self.connection).map_err(backend_error)
    }

    /// Lists live keys with a literal prefix, using SCAN instead of blocking KEYS.
    ///
    /// # Errors
    /// Returns an error when Redis rejects a scan.
    pub fn list_keys_by_prefix(&mut self, prefix: &str) -> FrameworkResult<Vec<String>> {
        let mut pattern = String::new();
        for character in prefix.chars() {
            if matches!(character, '*' | '?' | '[' | ']' | '\\') {
                pattern.push('\\');
            }
            pattern.push(character);
        }
        pattern.push('*');
        self.list_keys_matching(&pattern)
    }

    /// Lists sorted unique live keys using a Redis glob pattern and SCAN.
    /// TypeScript `listKeys(prefix)` corresponds to `format!("{prefix}*")`.
    /// SCAN is not a snapshot; concurrent key changes may affect results.
    ///
    /// # Errors
    /// Returns an error when Redis rejects a scan.
    pub fn list_keys_matching(&mut self, pattern: &str) -> FrameworkResult<Vec<String>> {
        let mut cursor = 0_u64;
        let mut keys = BTreeSet::new();
        loop {
            let (next, page): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(256)
                .query(&mut self.connection)
                .map_err(backend_error)?;
            keys.extend(page);
            cursor = next;
            if cursor == 0 {
                break;
            }
        }
        Ok(keys.into_iter().collect())
    }
}

impl TtlCache for RedisTtlCache {
    fn set(
        &mut self,
        key: impl Into<String>,
        value: Vec<u8>,
        ttl: Option<Duration>,
    ) -> FrameworkResult<()> {
        let key = normalize_key(key)?;
        let millis = ttl_millis(ttl.unwrap_or_else(|| self.config.default_ttl()))?;
        redis::cmd("SET")
            .arg(key)
            .arg(value)
            .arg("PX")
            .arg(millis)
            .query(&mut self.connection)
            .map_err(backend_error)
    }
    fn get(&mut self, key: &str) -> FrameworkResult<Option<Vec<u8>>> {
        redis::cmd("GET")
            .arg(normalize_key(key)?)
            .query(&mut self.connection)
            .map_err(backend_error)
    }
    fn delete(&mut self, key: &str) -> FrameworkResult<bool> {
        redis::cmd("DEL")
            .arg(normalize_key(key)?)
            .query(&mut self.connection)
            .map_err(backend_error)
    }
    fn peek(&mut self, key: &str) -> FrameworkResult<bool> {
        redis::cmd("EXISTS")
            .arg(normalize_key(key)?)
            .query(&mut self.connection)
            .map_err(backend_error)
    }
    fn peek_batch(&mut self, keys: &[String]) -> FrameworkResult<Vec<bool>> {
        let keys = keys
            .iter()
            .map(|key| normalize_key(key.as_str()))
            .collect::<FrameworkResult<Vec<_>>>()?;
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let mut pipeline = redis::pipe();
        pipeline.atomic();
        for key in keys {
            pipeline.cmd("EXISTS").arg(key);
        }
        pipeline.query(&mut self.connection).map_err(backend_error)
    }
    fn put_batch(
        &mut self,
        entries: Vec<(String, Vec<u8>)>,
        ttl: Option<Duration>,
    ) -> FrameworkResult<()> {
        let millis = ttl_millis(ttl.unwrap_or_else(|| self.config.default_ttl()))?;
        let entries: Vec<_> = entries
            .into_iter()
            .map(|(key, value)| Ok((normalize_key(key)?, value)))
            .collect::<FrameworkResult<_>>()?;
        if entries.is_empty() {
            return Ok(());
        }
        let mut pipeline = redis::pipe();
        pipeline.atomic();
        for (key, value) in entries {
            pipeline
                .cmd("SET")
                .arg(key)
                .arg(value)
                .arg("PX")
                .arg(millis)
                .ignore();
        }
        pipeline.query(&mut self.connection).map_err(backend_error)
    }
    fn get_batch(&mut self, keys: &[String]) -> FrameworkResult<Vec<Option<Vec<u8>>>> {
        let keys: Vec<_> = keys
            .iter()
            .map(|key| normalize_key(key.as_str()))
            .collect::<FrameworkResult<_>>()?;
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        redis::cmd("MGET")
            .arg(keys)
            .query(&mut self.connection)
            .map_err(backend_error)
    }
    fn delete_batch(&mut self, keys: &[String]) -> FrameworkResult<usize> {
        let keys: Vec<_> = keys
            .iter()
            .map(|key| normalize_key(key.as_str()))
            .collect::<FrameworkResult<_>>()?;
        if keys.is_empty() {
            return Ok(0);
        }
        redis::cmd("DEL")
            .arg(keys)
            .query(&mut self.connection)
            .map_err(backend_error)
    }
}

/// A shared FIFO queue backed by a Redis list. Dequeue removes an item atomically;
/// use a worker queue with receipts/leases when processing requires acknowledgments.
pub struct RedisQueue {
    key: String,
    connection: Connection,
    layout: RedisQueueLayout,
}

/// Select one layout consistently for every producer and consumer of a list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RedisQueueLayout {
    /// `LPUSH` / `RPOP`, matching the TypeScript Redis cache queue methods.
    #[default]
    TypeScript,
    /// `RPUSH` / `LPOP`, for lists written by the earlier native implementation.
    LegacyNative,
}

impl RedisQueue {
    /// Connect with the TypeScript-compatible `LPUSH` / `RPOP` FIFO layout.
    ///
    /// # Errors
    /// Returns an error for an empty key or connection failure.
    pub fn connect(url: &str, key: impl Into<String>) -> FrameworkResult<Self> {
        Self::connect_with_layout(url, key, RedisQueueLayout::default())
    }

    /// Preserve an existing native backlog by explicitly selecting `LegacyNative`.
    ///
    /// # Errors
    /// Returns an error for an empty key or connection failure.
    pub fn connect_with_layout(
        url: &str,
        key: impl Into<String>,
        layout: RedisQueueLayout,
    ) -> FrameworkResult<Self> {
        Ok(Self {
            key: normalize_key(key)?,
            connection: connect(url)?,
            layout,
        })
    }

    fn push_command(&self) -> &'static str {
        match self.layout {
            RedisQueueLayout::TypeScript => "LPUSH",
            RedisQueueLayout::LegacyNative => "RPUSH",
        }
    }

    fn pop_command(&self) -> &'static str {
        match self.layout {
            RedisQueueLayout::TypeScript => "RPOP",
            RedisQueueLayout::LegacyNative => "LPOP",
        }
    }

    fn query<T: FromRedisValue>(&mut self, command: &mut redis::Cmd) -> FrameworkResult<T> {
        command.query(&mut self.connection).map_err(backend_error)
    }

    /// # Errors
    /// Returns an error if Redis rejects the enqueue.
    pub fn enqueue(&mut self, value: &[u8]) -> FrameworkResult<()> {
        let _: usize = self.query(
            redis::cmd(self.push_command())
                .arg(self.key.clone())
                .arg(value),
        )?;
        Ok(())
    }
    /// # Errors
    /// Returns an error if Redis rejects the batch. Order matches input order.
    pub fn enqueue_batch(&mut self, values: &[Vec<u8>]) -> FrameworkResult<()> {
        if values.is_empty() {
            return Ok(());
        }
        let _: usize = self.query(
            redis::cmd(self.push_command())
                .arg(self.key.clone())
                .arg(values),
        )?;
        Ok(())
    }
    /// # Errors
    /// Returns an error if Redis rejects the dequeue.
    pub fn dequeue(&mut self) -> FrameworkResult<Option<Vec<u8>>> {
        self.query(redis::cmd(self.pop_command()).arg(self.key.clone()))
    }
    /// # Errors
    /// Returns an error if Redis rejects the batch dequeue (requires Redis 6.2+).
    pub fn dequeue_batch(&mut self, max: usize) -> FrameworkResult<Vec<Vec<u8>>> {
        if max == 0 {
            return Ok(Vec::new());
        }
        let values: Option<Vec<Vec<u8>>> = self.query(
            redis::cmd(self.pop_command())
                .arg(self.key.clone())
                .arg(max),
        )?;
        Ok(values.unwrap_or_default())
    }
    /// # Errors
    /// Returns an error if Redis rejects the peek.
    pub fn peek(&mut self) -> FrameworkResult<Option<Vec<u8>>> {
        let index = if self.layout == RedisQueueLayout::TypeScript {
            -1
        } else {
            0
        };
        self.query(redis::cmd("LINDEX").arg(self.key.clone()).arg(index))
    }
    /// Read the next items in dequeue order without removing them.
    ///
    /// # Errors
    /// Returns an error for an unsupported count or when Redis rejects the range.
    pub fn peek_batch(&mut self, max: usize) -> FrameworkResult<Vec<Vec<u8>>> {
        if max == 0 {
            return Ok(Vec::new());
        }
        let count = i64::try_from(max).map_err(|_| {
            FrameworkError::new(
                "redis_queue_invalid_count",
                "Queue count exceeds the Redis integer range",
            )
        })?;
        let (start, end) = if self.layout == RedisQueueLayout::TypeScript {
            (-count, -1)
        } else {
            (0, count - 1)
        };
        self.peek_range(start, end)
    }

    /// Reads the entire current list in dequeue order without removing items.
    /// This is the TypeScript zero-size peek behavior; ordinary `peek_batch(0)`
    /// continues to return an empty batch.
    ///
    /// # Errors
    /// Returns an error when Redis rejects the range.
    pub fn peek_all(&mut self) -> FrameworkResult<Vec<Vec<u8>>> {
        self.peek_range(0, -1)
    }

    fn peek_range(&mut self, start: i64, end: i64) -> FrameworkResult<Vec<Vec<u8>>> {
        let mut values: Vec<Vec<u8>> = self.query(
            redis::cmd("LRANGE")
                .arg(self.key.clone())
                .arg(start)
                .arg(end),
        )?;
        if self.layout == RedisQueueLayout::TypeScript {
            values.reverse();
        }
        Ok(values)
    }
    /// # Errors
    /// Returns an error if Redis rejects the length query.
    pub fn len(&mut self) -> FrameworkResult<usize> {
        self.query(redis::cmd("LLEN").arg(self.key.clone()))
    }
    /// # Errors
    /// Returns an error if Redis rejects the length query.
    pub fn is_empty(&mut self) -> FrameworkResult<bool> {
        Ok(self.len()? == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires FORKLAUNCH_TEST_REDIS_URL; exercised in container CI"]
    fn redis_backend_persists_expires_batches_and_consumes_fifo_across_clients() {
        let url = std::env::var("FORKLAUNCH_TEST_REDIS_URL").expect("Redis test URL required");
        let prefix = format!(
            "fl-parity:{}:{}:",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let config = RedisTtlCacheConfig::new(&url, Duration::from_secs(60));
        let mut writer = RedisTtlCache::connect(config.clone()).unwrap();
        let mut reader = RedisTtlCache::connect(config).unwrap();
        let key = format!("{prefix}binary");
        writer.set(&key, vec![0, 255, 1], None).unwrap();
        assert_eq!(reader.get(&key).unwrap(), Some(vec![0, 255, 1]));
        assert!(reader.peek(&key).unwrap());
        let expiry = format!("{prefix}expiry");
        writer
            .set(
                &expiry,
                b"expires".to_vec(),
                Some(Duration::from_millis(20)),
            )
            .unwrap();
        std::thread::sleep(Duration::from_millis(40));
        assert_eq!(reader.get(&expiry).unwrap(), None);
        assert!(writer.set(&key, vec![], Some(Duration::ZERO)).is_err());
        assert!(
            writer
                .set(&key, vec![], Some(Duration::from_nanos(1)))
                .is_err()
        );
        writer
            .put_batch(
                vec![
                    (format!("{prefix}a"), b"a".to_vec()),
                    (format!("{prefix}b"), b"b".to_vec()),
                ],
                None,
            )
            .unwrap();
        assert_eq!(
            reader
                .get_batch(&[
                    format!("{prefix}b"),
                    format!("{prefix}missing"),
                    format!("{prefix}a")
                ])
                .unwrap(),
            vec![Some(b"b".to_vec()), None, Some(b"a".to_vec())]
        );
        assert_eq!(reader.list_keys_by_prefix(&prefix).unwrap().len(), 3);
        let literal = format!("{prefix}[literal]*?");
        writer.set(&literal, vec![1], None).unwrap();
        assert_eq!(
            reader.list_keys_by_prefix(&literal).unwrap(),
            vec![literal.clone()]
        );
        // Invalid batches are rejected before any writes occur.
        let not_written = format!("{prefix}invalid-batch");
        assert!(
            writer
                .put_batch(
                    vec![(not_written.clone(), vec![1]), (" ".to_owned(), vec![2])],
                    None
                )
                .is_err()
        );
        assert_eq!(reader.get(&not_written).unwrap(), None);
        let keys = reader.list_keys_by_prefix(&prefix).unwrap();
        assert_eq!(reader.delete_batch(&keys).unwrap(), 4);
        assert!(!reader.delete(&key).unwrap());
        assert_eq!(
            reader.get_batch(&[]).unwrap(),
            Vec::<Option<Vec<u8>>>::new()
        );
        assert_eq!(reader.delete_batch(&[]).unwrap(), 0);
        writer.put_batch(vec![], None).unwrap();
        let queue_key = format!("{prefix}queue");
        let mut producer = RedisQueue::connect(&url, &queue_key).unwrap();
        let mut consumer = RedisQueue::connect(&url, &queue_key).unwrap();
        producer.enqueue(&[0, 255]).unwrap();
        producer
            .enqueue_batch(&[b"second".to_vec(), b"third".to_vec()])
            .unwrap();
        assert_eq!(consumer.peek().unwrap(), Some(vec![0, 255]));
        assert_eq!(consumer.peek_batch(0).unwrap(), Vec::<Vec<u8>>::new());
        assert_eq!(
            consumer.peek_batch(2).unwrap(),
            vec![vec![0, 255], b"second".to_vec()]
        );
        assert_eq!(consumer.len().unwrap(), 3);
        assert_eq!(consumer.dequeue_batch(0).unwrap(), Vec::<Vec<u8>>::new());
        assert_eq!(consumer.dequeue().unwrap(), Some(vec![0, 255]));
        assert_eq!(
            consumer.dequeue_batch(10).unwrap(),
            vec![b"second".to_vec(), b"third".to_vec()]
        );
        assert!(consumer.is_empty().unwrap());
        assert_eq!(consumer.dequeue().unwrap(), None);
        assert!(consumer.dequeue_batch(1).unwrap().is_empty());
    }
}
