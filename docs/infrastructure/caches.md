---
title: "Caches"
description: "Redis-based TTL caching system for key-value storage, queue operations, and batch processing."
category: "Infrastructure"
---

## Overview

ForkLaunch provides a TTL (Time-To-Live) caching system through `@forklaunch/core/cache` with a production-ready Redis implementation in `@forklaunch/infrastructure-redis`. The cache supports key-value storage, queue operations (FIFO), batch processing, and automatic expiration.

In production, the cache runs on Amazon ElastiCache. Locally, it runs in a Redis Docker container.

## Quick Start

```typescript
import { RedisTtlCache } from '@forklaunch/infrastructure-redis';

const cache = new RedisTtlCache(
  60000, // Default TTL: 60 seconds
  openTelemetryCollector,
  { url: 'redis://localhost:6379' },
  { enabled: { logging: true, metrics: true, tracing: true } }
);

// Store a value
await cache.putRecord({
  key: 'user:123',
  value: { name: 'Alice', email: 'alice@example.com' },
  ttlMilliseconds: 300000 // 5 minutes
});

// Read it back
const record = await cache.readRecord('user:123');
console.log(record.value); // { name: 'Alice', email: 'alice@example.com' }
```

## Core Operations

### Key-Value Operations

| Method | Description |
|--------|-------------|
| `putRecord(record)` | Store a value with TTL |
| `readRecord(key)` | Retrieve a value (throws if missing) |
| `deleteRecord(key)` | Remove a value |
| `peekRecord(key)` | Check if a key exists (returns boolean) |

### Batch Operations

| Method | Description |
|--------|-------------|
| `putBatchRecords(records)` | Store multiple values |
| `readBatchRecords(keys \| prefix)` | Read multiple values by key array or prefix |
| `deleteBatchRecords(keys)` | Remove multiple values |
| `peekBatchRecords(keys \| prefix)` | Check existence of multiple keys |

### Queue Operations (FIFO)

| Method | Description |
|--------|-------------|
| `enqueueRecord(queue, value)` | Add to queue (left push) |
| `dequeueRecord(queue)` | Remove from queue (right pop, throws if empty) |
| `peekQueueRecord(queue)` | Look at next item without removing |
| `enqueueBatchRecords(queue, values)` | Add multiple items |
| `dequeueBatchRecords(queue, count)` | Remove multiple items |

## Cache Keys

Use `createCacheKey` for consistent key naming:

```typescript
import { createCacheKey } from '@forklaunch/core/cache';

const createUserKey = createCacheKey('user');
const createSessionKey = createCacheKey('session');

createUserKey('123');    // 'user:123'
createSessionKey('abc'); // 'session:abc'
```

## Common Patterns

### Cached Database Queries

```typescript
async function getCachedUser(userId: string): Promise<User> {
  const cacheKey = `user:${userId}`;
  try {
    const record = await cache.readRecord<User>(cacheKey);
    return record.value;
  } catch {
    const user = await em.findOneOrFail(User, { id: userId });
    await cache.putRecord({
      key: cacheKey,
      value: user,
      ttlMilliseconds: 300000
    });
    return user;
  }
}
```

### Feature Gating Cache

The platform caches billing features per organization to avoid repeated HMAC calls:

```typescript
// billingCacheService caches features from the billing module
const features = await billingCacheService.getCachedFeatures(organizationId);
// Returns Set<string> of active feature names
```

### Rate Limiting

```typescript
async function checkRateLimit(userId: string, limit = 100): Promise<boolean> {
  const key = `ratelimit:${userId}`;
  try {
    const record = await cache.readRecord<number>(key);
    if (record.value >= limit) return false;
    await cache.putRecord({
      key,
      value: record.value + 1,
      ttlMilliseconds: record.ttlMilliseconds
    });
  } catch {
    await cache.putRecord({ key, value: 1, ttlMilliseconds: 60000 });
  }
  return true;
}
```

## Best Practices

1. **Always set explicit TTLs** don't rely on the default
2. **Use `createCacheKey`** for consistent key naming with prefixes
3. **Handle cache misses gracefully** `readRecord` throws on missing keys
4. **Use batch operations** for multiple keys to reduce round trips
5. **Call `cache.disconnect()`** during graceful shutdown
6. **Type your cache reads** `cache.readRecord<User>(key)` for type safety

## Environment Variables

```bash
REDIS_URL=redis://localhost:6379
```

In production, this points to your ElastiCache cluster.

## Related Documentation

- [Cache Guide](/docs/guides/cache.md): Full API reference with detailed examples
- [Infrastructure Overview](/docs/infrastructure/overview.md)
- [Queues](/docs/infrastructure/queues.md)
- [Testing Guide](/docs/guides/testing.md): Testing with Redis containers
