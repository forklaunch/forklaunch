---
title: Cache Module
category: Framework
description: Complete guide to using the TTL Cache system with Redis implementation.
---

## Overview

ForkLaunch provides a powerful TTL (Time-To-Live) caching system through the `@forklaunch/core/cache` module with a production-ready Redis implementation in `@forklaunch/infrastructure-redis`. The cache supports key-value storage, queue operations, batch processing, and automatic expiration.

## Quick Start

### Installation

```bash
# Core cache interface
pnpm add @forklaunch/core

# Redis implementation
pnpm add @forklaunch/infrastructure-redis redis
```

### Basic Usage

```typescript
import { RedisTtlCache } from '@forklaunch/infrastructure-redis';
import { OpenTelemetryCollector } from '@forklaunch/core/http';

// Create a cache instance
const cache = new RedisTtlCache(
  60000, // Default TTL: 60 seconds
  openTelemetryCollector,
  {
    url: 'redis://localhost:6379'
  },
  {
    enabled: {
      logging: true,
      metrics: true,
      tracing: true
    }
  }
);

// Put a record
await cache.putRecord({
  key: 'user:123',
  value: { name: 'Alice', email: 'alice@example.com' },
  ttlMilliseconds: 60000
});

// Read a record
const record = await cache.readRecord('user:123');
console.log(record.value); // { name: 'Alice', email: 'alice@example.com' }

// Delete a record
await cache.deleteRecord('user:123');
```

## Core Concepts

### TtlCacheRecord

Every cache entry is a `TtlCacheRecord` with three properties:

```typescript
type TtlCacheRecord<T> = {
  key: string;           // Unique identifier
  value: T;              // Any JSON-serializable data
  ttlMilliseconds: number; // Time-to-live in milliseconds
};
```

### CacheKey Utility

Generate consistent cache keys with prefixes:

```typescript
import { createCacheKey } from '@forklaunch/core/cache';

const createUserKey = createCacheKey('user');
const createSessionKey = createCacheKey('session');

const userKey = createUserKey('123'); // 'user:123'
const sessionKey = createSessionKey('abc'); // 'session:abc'
```

## API Reference

### Basic Operations

#### putRecord

Store a single value in the cache.

```typescript
await cache.putRecord<User>({
  key: 'user:123',
  value: { id: '123', name: 'Alice' },
  ttlMilliseconds: 300000 // 5 minutes
});
```

#### readRecord

Retrieve a value from the cache.

```typescript
const record = await cache.readRecord<User>('user:123');
console.log(record.key);            // 'user:123'
console.log(record.value);          // { id: '123', name: 'Alice' }
console.log(record.ttlMilliseconds); // Remaining TTL in ms
```

**Throws:** Error if the key doesn't exist.

#### deleteRecord

Remove a value from the cache.

```typescript
await cache.deleteRecord('user:123');
```

#### peekRecord

Check if a key exists without reading its value.

```typescript
const exists = await cache.peekRecord('user:123');
console.log(exists); // true or false
```

### Batch Operations

Process multiple records in a single transaction for better performance.

#### putBatchRecords

```typescript
await cache.putBatchRecords([
  { key: 'user:1', value: { name: 'Alice' }, ttlMilliseconds: 60000 },
  { key: 'user:2', value: { name: 'Bob' }, ttlMilliseconds: 60000 },
  { key: 'user:3', value: { name: 'Charlie' }, ttlMilliseconds: 60000 }
]);
```

#### readBatchRecords

Read multiple records by keys or prefix:

```typescript
// By explicit keys
const records = await cache.readBatchRecords(['user:1', 'user:2', 'user:3']);

// By prefix (pattern matching)
const allUsers = await cache.readBatchRecords('user:');
// Returns all records with keys starting with 'user:'
```

#### deleteBatchRecords

```typescript
await cache.deleteBatchRecords(['user:1', 'user:2', 'user:3']);
```

#### peekBatchRecords

```typescript
// Check specific keys
const existence = await cache.peekBatchRecords(['user:1', 'user:2']);
console.log(existence); // [true, false]

// Check by prefix
const allUserKeys = await cache.peekBatchRecords('user:');
console.log(allUserKeys); // [true, true, true, ...]
```

### Queue Operations

The cache supports Redis list-based queue operations (FIFO).

#### enqueueRecord

Add an item to the queue (left push):

```typescript
await cache.enqueueRecord('notifications', {
  userId: '123',
  message: 'Welcome!',
  timestamp: Date.now()
});
```

#### dequeueRecord

Remove and return an item from the queue (right pop):

```typescript
const notification = await cache.dequeueRecord<Notification>('notifications');
console.log(notification); // { userId: '123', message: 'Welcome!', ... }
```

**Throws:** Error if the queue is empty.

#### peekQueueRecord

Look at the next item without removing it:

```typescript
const next = await cache.peekQueueRecord<Notification>('notifications');
// Item remains in queue
```

#### enqueueBatchRecords

Add multiple items to the queue:

```typescript
await cache.enqueueBatchRecords('notifications', [
  { userId: '1', message: 'Message 1' },
  { userId: '2', message: 'Message 2' },
  { userId: '3', message: 'Message 3' }
]);
```

#### dequeueBatchRecords

Remove and return multiple items:

```typescript
const items = await cache.dequeueBatchRecords<Notification>('notifications', 10);
console.log(items.length); // Up to 10 items
```

#### peekQueueRecords

Look at multiple items without removing them:

```typescript
const nextTen = await cache.peekQueueRecords<Notification>('notifications', 10);
// Items remain in queue
```

### Utility Methods

#### listKeys

List all keys matching a prefix:

```typescript
const userKeys = await cache.listKeys('user:');
console.log(userKeys); // ['user:1', 'user:2', 'user:3', ...]
```

#### getTtlMilliseconds

Get the default TTL for the cache instance:

```typescript
const defaultTtl = cache.getTtlMilliseconds();
console.log(defaultTtl); // 60000
```

#### disconnect

Gracefully close the Redis connection:

```typescript
await cache.disconnect();
```

## Common Patterns

### Session Management

```typescript
import { createCacheKey } from '@forklaunch/core/cache';

const createSessionKey = createCacheKey('session');

// Store session
async function createSession(userId: string, sessionData: Session) {
  const sessionId = generateSessionId();
  await cache.putRecord({
    key: createSessionKey(sessionId),
    value: { userId, ...sessionData },
    ttlMilliseconds: 86400000 // 24 hours
  });
  return sessionId;
}

// Retrieve session
async function getSession(sessionId: string): Promise<Session | null> {
  try {
    const record = await cache.readRecord<Session>(createSessionKey(sessionId));
    return record.value;
  } catch (error) {
    return null; // Session expired or doesn't exist
  }
}

// Extend session
async function refreshSession(sessionId: string) {
  const record = await cache.readRecord<Session>(createSessionKey(sessionId));
  await cache.putRecord({
    ...record,
    ttlMilliseconds: 86400000 // Reset to 24 hours
  });
}
```

### Rate Limiting

```typescript
async function checkRateLimit(userId: string, limit: number = 100): Promise<boolean> {
  const key = `ratelimit:${userId}`;

  try {
    const record = await cache.readRecord<number>(key);
    if (record.value >= limit) {
      return false; // Rate limit exceeded
    }

    // Increment counter
    await cache.putRecord({
      key,
      value: record.value + 1,
      ttlMilliseconds: record.ttlMilliseconds
    });
  } catch (error) {
    // First request in window
    await cache.putRecord({
      key,
      value: 1,
      ttlMilliseconds: 60000 // 1 minute window
    });
  }

  return true;
}
```

### Job Queue with Priority

```typescript
interface Job {
  id: string;
  priority: 'high' | 'normal' | 'low';
  payload: any;
}

async function enqueueJob(job: Job) {
  const queueName = `jobs:${job.priority}`;
  await cache.enqueueRecord(queueName, job);
}

async function processJobs() {
  // Try high priority first
  try {
    const job = await cache.dequeueRecord<Job>('jobs:high');
    await processJob(job);
    return;
  } catch {}

  // Then normal priority
  try {
    const job = await cache.dequeueRecord<Job>('jobs:normal');
    await processJob(job);
    return;
  } catch {}

  // Finally low priority
  try {
    const job = await cache.dequeueRecord<Job>('jobs:low');
    await processJob(job);
  } catch {
    // No jobs available
  }
}
```

### Cached Database Queries

```typescript
async function getCachedUser(userId: string): Promise<User> {
  const cacheKey = `user:${userId}`;

  // Try cache first
  try {
    const record = await cache.readRecord<User>(cacheKey);
    return record.value;
  } catch {
    // Cache miss - fetch from database
    const user = await db.users.findOne({ id: userId });

    // Store in cache
    await cache.putRecord({
      key: cacheKey,
      value: user,
      ttlMilliseconds: 300000 // 5 minutes
    });

    return user;
  }
}

// Invalidate cache on update
async function updateUser(userId: string, updates: Partial<User>) {
  await db.users.update({ id: userId }, updates);
  await cache.deleteRecord(`user:${userId}`);
}
```

### Batch Processing

```typescript
async function cacheMultipleUsers(users: User[]) {
  const records = users.map(user => ({
    key: `user:${user.id}`,
    value: user,
    ttlMilliseconds: 300000
  }));

  await cache.putBatchRecords(records);
}

async function getAllCachedUsers(): Promise<User[]> {
  const records = await cache.readBatchRecords<User>('user:');
  return records.map(record => record.value);
}
```

## Redis-Specific Features

### Connection Options

```typescript
const cache = new RedisTtlCache(
  60000,
  openTelemetryCollector,
  {
    url: 'redis://localhost:6379',
    // or
    socket: {
      host: 'localhost',
      port: 6379,
      tls: true,
      reconnectStrategy: (retries) => Math.min(retries * 50, 500)
    },
    username: 'default',
    password: process.env.REDIS_PASSWORD,
    database: 0
  },
  telemetryOptions
);
```

### Error Handling

The Redis client automatically logs errors to OpenTelemetry when telemetry is enabled:

```typescript
const cache = new RedisTtlCache(
  60000,
  openTelemetryCollector,
  redisOptions,
  {
    enabled: {
      logging: true,  // Enables error logging
      metrics: true,
      tracing: true
    }
  }
);
```

### Performance Optimization

1. **Use batch operations** for multiple keys
2. **Set appropriate TTLs** to avoid memory bloat
3. **Use prefix patterns** for efficient key scanning
4. **Enable pipelining** through batch methods
5. **Monitor with telemetry** to track cache hit rates

## Best Practices

### 1. Always Set Appropriate TTLs

```typescript
// ✅ Good - explicit TTL
await cache.putRecord({
  key: 'user:123',
  value: user,
  ttlMilliseconds: 300000 // 5 minutes
});

// ❌ Avoid - relies on default TTL
await cache.putRecord({
  key: 'user:123',
  value: user,
  ttlMilliseconds: cache.getTtlMilliseconds()
});
```

### 2. Handle Cache Misses Gracefully

```typescript
async function getUser(id: string): Promise<User | null> {
  try {
    const record = await cache.readRecord<User>(`user:${id}`);
    return record.value;
  } catch (error) {
    // Don't throw - return null or fetch from DB
    return await fetchUserFromDb(id);
  }
}
```

### 3. Use Consistent Key Naming

```typescript
// ✅ Good - consistent prefix with createCacheKey
const createUserKey = createCacheKey('user');
const createSessionKey = createCacheKey('session');
const createProductKey = createCacheKey('product');

// ❌ Avoid - inconsistent manual keys
const key1 = 'user_123';
const key2 = 'user:456';
const key3 = 'users/789';
```

### 4. Clean Up Resources

```typescript
// In your application shutdown logic
process.on('SIGTERM', async () => {
  await cache.disconnect();
  process.exit(0);
});
```

### 5. Use Type Safety

```typescript
interface UserCache {
  id: string;
  name: string;
  email: string;
}

// ✅ Type-safe operations
const record = await cache.readRecord<UserCache>('user:123');
const user: UserCache = record.value; // TypeScript validates structure

// ❌ Avoid - untyped
const record = await cache.readRecord('user:123');
const user = record.value; // any type
```

## Testing

### Using In-Memory Cache for Tests

For testing, you can create a mock implementation of `TtlCache`:

```typescript
import { TtlCache, TtlCacheRecord } from '@forklaunch/core/cache';

class InMemoryCache implements TtlCache {
  private store = new Map<string, TtlCacheRecord<any>>();

  async putRecord<T>(record: TtlCacheRecord<T>): Promise<void> {
    this.store.set(record.key, record);
  }

  async readRecord<T>(key: string): Promise<TtlCacheRecord<T>> {
    const record = this.store.get(key);
    if (!record) throw new Error(`Record not found: ${key}`);
    return record;
  }

  async deleteRecord(key: string): Promise<void> {
    this.store.delete(key);
  }

  // ... implement other methods
}

// In your tests
const cache = new InMemoryCache();
```

### Using TestContainers

```typescript
import { TestContainerManager } from '@forklaunch/testing';

const manager = new TestContainerManager();
const redisContainer = await manager.startRedis();

const cache = new RedisTtlCache(
  60000,
  openTelemetryCollector,
  {
    url: redisContainer.getConnectionUrl()
  },
  telemetryOptions
);

// Run tests...

await manager.stopAll();
```

## Related Documentation

- [Object Store](/docs/guides/objectstore.md): For large binary/file storage
- [Testing](/docs/guides/testing.md): TestContainers setup for Redis
- [Telemetry](/docs/framework/telemetry.md): Monitoring cache performance
- [Config Injector](/docs/framework/config.md): Configuration management
