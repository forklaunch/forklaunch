//! Redis is synchronous; call `connect` and cache operations on a blocking executor.
use std::time::Duration;

use forklaunch_core::{common::FrameworkResult, config, persistence::json::JsonRecordCodec};
use forklaunch_infrastructure_redis::{RedisTtlCacheConfig, client::{RedisTtlCache, RedisQueue}, json::{JsonCache, JsonQueue}};

pub fn config() -> FrameworkResult<RedisTtlCacheConfig> {
    let url: String = config::required("REDIS_URL")?;
    let config = RedisTtlCacheConfig::new(url, Duration::from_secs(60 * 60));
    config.validate()?;
    Ok(config)
}

/// Connect explicitly when the application needs Redis; service startup stays independent.
pub fn connect() -> FrameworkResult<RedisTtlCache> {
    RedisTtlCache::connect(config()?)
}

/// Explicit encrypted JSON factory; ENCRYPTION_KEY is only required when this is called.
pub fn encrypted_cache()
-> FrameworkResult<JsonCache<RedisTtlCache>> {
    let codec = JsonRecordCodec::from_env()?;
    Ok(JsonCache::new(connect()?, codec))
}

/// Explicit encrypted FIFO queue factory sharing the configured Redis URL/key ring.
pub fn encrypted_queue(
    name: &str,
) -> FrameworkResult<JsonQueue> {
    let codec = JsonRecordCodec::from_env()?;
    let queue = RedisQueue::connect(&config()?.url, name)?;
    Ok(JsonQueue::new(queue, codec))
}
