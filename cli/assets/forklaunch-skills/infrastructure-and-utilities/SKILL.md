---
name: infrastructure-and-utilities
description: "Infra: Redis cache, S3 object store, TestContainers, utilities."
user-invokable: true
---

# ForkLaunch Infrastructure & Utilities Skill

## When to Use This Skill

Use this skill when the user asks to:

- Implement caching with Redis (RedisTtlCache)
- Store large files or documents (S3ObjectStore)
- Set up integration tests with TestContainers
- Use utility functions for string manipulation, object operations, or type guards
- Work with BaseEntity for CRUD operations
- Set up cascading environment variable loading
- Transform data with mappers (requestMapper, responseMapper)

## Cache Pattern (TTL-based with Redis)

### Overview

ForkLaunch provides `@forklaunch/core/cache` with a `TtlCache` interface and `RedisTtlCache` implementation for short-term data storage with automatic expiration.

**When to use Cache vs Object Store:**

- **Cache**: Small data (<1MB), temporary, needs fast access, OK to lose
  - Examples: Sessions, rate limits, cached DB queries, temporary tokens
- **Object Store**: Large files (>1MB), permanent, documents, user uploads
  - Examples: Profile pictures, documents, backups, large datasets

### Basic Cache Usage

```typescript
import {
  createCacheKey,
  TtlCache,
  TtlCacheRecord,
} from "@forklaunch/core/cache";
import { RedisTtlCache } from "@forklaunch/infrastructure-redis";

// Create cache with 60-second default TTL
const cache = new RedisTtlCache(
  60000, // 60 seconds
  openTelemetryCollector,
  { url: process.env.REDIS_URL },
);

// Create typed cache key functions
const createUserCacheKey = createCacheKey("user");
const createSessionKey = createCacheKey("session");

// Put record with custom TTL (5 minutes)
await cache.putRecord({
  key: createUserCacheKey("user-123"),
  value: { name: "Alice", email: "alice@example.com", roles: ["admin"] },
  ttlMilliseconds: 300000,
});

// Read record (returns null if expired or not found)
const user = await cache.readRecord<UserData>(createUserCacheKey("user-123"));

// Delete record
await cache.deleteRecord(createUserCacheKey("user-123"));

// Peek without extending TTL
const value = await cache.peekRecord(createUserCacheKey("user-123"));
```

### Cache Patterns

#### 1. Session Management

```typescript
const createSessionKey = createCacheKey("session");

// Store session (30 minutes)
await cache.putRecord({
  key: createSessionKey(sessionId),
  value: { userId: "123", permissions: ["read", "write"] },
  ttlMilliseconds: 1800000,
});

// Read session
const session = await cache.readRecord(createSessionKey(sessionId));
if (!session) {
  return res.status(401).json({ error: "Session expired" });
}
```

#### 2. Rate Limiting

```typescript
const createRateLimitKey = createCacheKey("rate-limit");

async function checkRateLimit(userId: string, limit: number, windowMs: number) {
  const key = createRateLimitKey(userId);
  const record = await cache.readRecord<{ count: number }>(key);

  if (record && record.count >= limit) {
    return false; // Rate limit exceeded
  }

  await cache.putRecord({
    key,
    value: { count: (record?.count || 0) + 1 },
    ttlMilliseconds: windowMs,
  });

  return true;
}
```

#### 3. Cached Database Queries

```typescript
const createQueryCacheKey = createCacheKey("query");

async function getCachedUsers(filters: UserFilters) {
  const cacheKey = createQueryCacheKey(JSON.stringify(filters));

  // Try cache first
  const cached = await cache.readRecord<User[]>(cacheKey);
  if (cached) return cached;

  // Query database
  const users = await em.find(User, filters);

  // Cache for 5 minutes
  await cache.putRecord({
    key: cacheKey,
    value: users,
    ttlMilliseconds: 300000,
  });

  return users;
}
```

#### 4. Queue Operations (FIFO)

```typescript
// Enqueue items
await cache.enqueueRecord({
  queueKey: "email-queue",
  value: { to: "user@example.com", subject: "Welcome", body: "..." },
});

// Dequeue and process
const email = await cache.dequeueRecord<EmailJob>("email-queue");
if (email) {
  await sendEmail(email);
}

// Peek at queue without removing
const nextItem = await cache.peekQueueRecord<EmailJob>("email-queue");
```

### Batch Operations

```typescript
// Put multiple records at once
await cache.putRecordBatch([
  {
    key: createUserCacheKey("user-1"),
    value: { name: "Alice" },
    ttlMilliseconds: 300000,
  },
  {
    key: createUserCacheKey("user-2"),
    value: { name: "Bob" },
    ttlMilliseconds: 300000,
  },
]);

// Read multiple records
const users = await cache.readRecordBatch([
  createUserCacheKey("user-1"),
  createUserCacheKey("user-2"),
]);

// Delete multiple records
await cache.deleteRecordBatch([
  createUserCacheKey("user-1"),
  createUserCacheKey("user-2"),
]);
```

## Object Store Pattern (S3 for Large Files)

### Overview

ForkLaunch provides `@forklaunch/core/objectstore` with an `ObjectStore` interface and `S3ObjectStore` implementation for large file storage.

### Basic Object Store Usage

```typescript
import {
  createObjectStoreKey,
  ObjectStore,
} from "@forklaunch/core/objectstore";
import { S3ObjectStore } from "@forklaunch/infrastructure-s3";

// Create object store
const objectStore = new S3ObjectStore({
  region: "us-east-1",
  bucketName: "my-app-uploads",
  openTelemetryCollector,
});

// Create typed key functions
const createUserFileKey = createObjectStoreKey("user-files");
const createDocumentKey = createObjectStoreKey("documents");

// Upload object
await objectStore.putObject({
  key: createUserFileKey("user-123", "avatar.png"),
  value: imageBuffer,
  metadata: {
    contentType: "image/png",
    userId: "user-123",
  },
});

// Download object
const file = await objectStore.readObject(
  createUserFileKey("user-123", "avatar.png"),
);
// Returns: { value: Buffer, metadata: { contentType, userId } }

// Delete object
await objectStore.deleteObject(createUserFileKey("user-123", "avatar.png"));
```

### Object Store Patterns

#### 1. User File Uploads

```typescript
const createUserFileKey = createObjectStoreKey("user-files");

async function uploadUserFile(userId: string, file: Express.Multer.File) {
  const key = createUserFileKey(userId, file.originalname);

  await objectStore.putObject({
    key,
    value: file.buffer,
    metadata: {
      contentType: file.mimetype,
      size: file.size,
      uploadedAt: new Date().toISOString(),
    },
  });

  return { fileKey: key, url: `/files/${userId}/${file.originalname}` };
}
```

#### 2. Document Versioning

```typescript
const createDocVersionKey = createObjectStoreKey("documents");

async function saveDocumentVersion(
  docId: string,
  content: Buffer,
  version: number,
) {
  await objectStore.putObject({
    key: createDocVersionKey(docId, `v${version}`),
    value: content,
    metadata: {
      docId,
      version: version.toString(),
      createdAt: new Date().toISOString(),
    },
  });
}

async function getLatestVersion(docId: string): Promise<number> {
  // List objects with prefix and find highest version
  const versions = await listDocumentVersions(docId);
  return Math.max(...versions.map((v) => v.version));
}
```

#### 3. Streaming Large Files

```typescript
// Stream download (memory efficient for large files)
app.get("/files/:userId/:filename", async (req, res) => {
  const key = createUserFileKey(req.params.userId, req.params.filename);

  const stream = await objectStore.streamDownloadObject(key);

  res.setHeader("Content-Type", "application/octet-stream");
  res.setHeader(
    "Content-Disposition",
    `attachment; filename="${req.params.filename}"`,
  );

  stream.pipe(res);
});

// Stream upload
app.post("/files/upload", async (req, res) => {
  const uploadStream = objectStore.streamUploadObject({
    key: createUserFileKey("user-123", "large-file.zip"),
    metadata: { contentType: "application/zip" },
  });

  req.pipe(uploadStream);

  uploadStream.on("finish", () => {
    res.json({ message: "Upload complete" });
  });
});
```

#### 4. Batch Operations

```typescript
// Upload multiple files
await objectStore.putObjectBatch([
  {
    key: createUserFileKey("user-123", "photo1.jpg"),
    value: photo1Buffer,
    metadata: { contentType: "image/jpeg" },
  },
  {
    key: createUserFileKey("user-123", "photo2.jpg"),
    value: photo2Buffer,
    metadata: { contentType: "image/jpeg" },
  },
]);

// Download multiple files
const files = await objectStore.readObjectBatch([
  createUserFileKey("user-123", "photo1.jpg"),
  createUserFileKey("user-123", "photo2.jpg"),
]);

// Delete multiple files
await objectStore.deleteObjectBatch([
  createUserFileKey("user-123", "photo1.jpg"),
  createUserFileKey("user-123", "photo2.jpg"),
]);
```

## Testing with TestContainers

### Overview

ForkLaunch provides `@forklaunch/testing` with `TestContainerManager` and `BlueprintTestHarness` for integration testing with real Docker containers (PostgreSQL, MySQL, MongoDB, Redis, Kafka, S3).

### Basic Integration Test Setup

```typescript
import {
  BlueprintTestHarness,
  TEST_TOKENS,
  clearTestDatabase,
} from "@forklaunch/testing";
import type { TestSetupResult } from "@forklaunch/testing";

describe("User API Integration Tests", () => {
  let harness: BlueprintTestHarness;
  let setup: TestSetupResult;

  beforeAll(async () => {
    harness = new BlueprintTestHarness({
      getConfig: async () => {
        const { default: config } = await import("../mikro-orm.config");
        return config;
      },
      databaseType: "postgres",
      useMigrations: false, // Fast: use schema generation
      needsRedis: true,
      needsS3: true,
      s3Bucket: "test-uploads",
    });

    setup = await harness.setup();
  }, 60000); // 60s timeout for container startup

  afterAll(async () => {
    await harness.cleanup();
  }, 30000);

  beforeEach(async () => {
    // Clear database for test isolation
    await clearTestDatabase({ orm: setup.orm });

    // Seed test data
    const em = setup.orm!.em.fork();
    em.create(User, {
      id: "123",
      email: "test@example.com",
      name: "Test User",
    });
    await em.flush();
  });

  it("should create user with AUTH token", async () => {
    const response = await createUserRoute.sdk.createUser({
      body: {
        email: "new@example.com",
        name: "New User",
      },
      headers: {
        authorization: TEST_TOKENS.AUTH,
      },
    });

    expect(response.code).toBe(201);
    expect(response.response.email).toBe("new@example.com");
  });

  it("should require authentication", async () => {
    const response = await createUserRoute.sdk.createUser({
      body: { email: "test@example.com", name: "Test" },
      headers: {}, // No auth token
    });

    expect(response.code).toBe(401);
  });
});
```

### Test Tokens

```typescript
import { TEST_TOKENS } from "@forklaunch/testing";

// Standard authentication token
headers: {
  authorization: TEST_TOKENS.AUTH;
}

// HMAC authentication token
headers: {
  authorization: TEST_TOKENS.HMAC;
}

// Invalid HMAC token (for testing error cases)
headers: {
  authorization: TEST_TOKENS.HMAC_INVALID;
}
```

### Testing with Multiple Services

```typescript
const harness = new BlueprintTestHarness({
  getConfig: async () => {
    const { default: config } = await import("../mikro-orm.config");
    return config;
  },
  databaseType: "postgres",
  needsRedis: true,
  needsKafka: true,
  needsS3: true,
  s3Bucket: "test-uploads",
  customEnvVars: {
    API_KEY: "test-api-key",
    EXTERNAL_SERVICE_URL: "http://localhost:3001",
  },
  onSetup: async (setup) => {
    // Custom setup after containers are ready
    console.log("Redis:", process.env.REDIS_URL);
    console.log("S3:", process.env.S3_ENDPOINT);
    console.log("Kafka:", process.env.KAFKA_BROKERS);
  },
});

const setup = await harness.setup();

// All services available:
// - setup.orm (PostgreSQL ORM)
// - setup.redis (Redis client)
// - setup.kafkaContainer (Kafka container)
// - setup.s3Container (LocalStack S3 container)
```

## Utility Functions

### String Manipulation

```typescript
import {
  toCamelCaseIdentifier,
  toPrettyCamelCase,
  capitalize,
  uncapitalize,
  isValidIdentifier,
} from "@forklaunch/common";

// Convert to camelCase identifier
toCamelCaseIdentifier("hello-world"); // 'helloWorld'
toCamelCaseIdentifier("my_var_name"); // 'myVarName'
toCamelCaseIdentifier("API-Key"); // 'aPIKey'

// Pretty camelCase (lowercases abbreviations first)
toPrettyCamelCase("API-Key"); // 'apiKey'
toPrettyCamelCase("HTTP-Response"); // 'httpResponse'
toPrettyCamelCase("user-ID"); // 'userId'

// Capitalize/uncapitalize
capitalize("hello"); // 'Hello'
uncapitalize("Hello"); // 'hello'

// Validate identifier
isValidIdentifier("myVar123"); // true
isValidIdentifier("123invalid"); // false
isValidIdentifier("my-var"); // false
```

### Object Utilities

```typescript
import {
  stripUndefinedProperties,
  deepCloneWithoutUndefined,
  sortObjectKeys,
  toRecord,
  isRecord,
} from "@forklaunch/common";

// Remove undefined properties (shallow)
const obj = { a: 1, b: undefined, c: 3 };
stripUndefinedProperties(obj); // { a: 1, c: 3 }

// Deep clone without undefined
const cloned = deepCloneWithoutUndefined(obj);

// Sort object keys (useful for consistent serialization)
const sorted = sortObjectKeys({ c: 3, a: 1, b: 2 });
// { a: 1, b: 2, c: 3 }

// Type guard for objects
if (isRecord(value)) {
  // TypeScript knows value is Record<string, unknown>
  const keys = Object.keys(value);
}
```

### Hashing

```typescript
import { hashString } from "@forklaunch/common";

// SHA-256 hash
const hash = hashString("my-secret-string");
// Returns hex string: 'a1b2c3d4...'
```

### Persistence Utilities

```typescript
import { BaseEntity } from "@forklaunch/core/persistence";
import { Entity, PrimaryKey, Property } from "@mikro-orm/core";

@Entity()
class User extends BaseEntity {
  @PrimaryKey()
  id!: string;

  @Property()
  name!: string;

  @Property()
  email!: string;
}

// Create entity (with EntityManager - recommended)
const user = await User.create(
  {
    id: "123",
    name: "Alice",
    email: "alice@example.com",
  },
  em,
);

// Update entity
const updated = await User.update(
  {
    id: "123",
    name: "Alice Updated",
  },
  em,
);
await em.flush();

// Read as DTO (plain object without ORM metadata)
const userDto = await user.read(em);
// Returns: { id: '123', name: 'Alice', email: 'alice@example.com' }
```

### Mappers

```typescript
import { requestMapper, responseMapper } from "@forklaunch/core/mappers";
import { SchemaValidator, string, number } from "@forklaunch/validator/zod";

const validator = SchemaValidator();

// Request mapper (DTO → Entity)
const createUserMapper = requestMapper({
  schemaValidator: validator,
  schema: {
    name: string,
    age: number,
  },
  entity: User,
  mapperDefinition: {
    toEntity: async (dto) => {
      return new User(dto.name, dto.age);
    },
  },
});

// Response mapper (Entity → DTO)
const userResponseMapper = responseMapper({
  schemaValidator: validator,
  schema: {
    id: string,
    name: string,
    age: number,
  },
  entity: User,
  mapperDefinition: {
    toDto: async (entity) => {
      return {
        id: entity.id,
        name: entity.name,
        age: entity.age,
      };
    },
  },
});

// Usage
const entity = await createUserMapper.toEntity({ name: "Alice", age: 30 });
const dto = await userResponseMapper.toDto(entity);
```

### Environment Variables

```typescript
import { loadCascadingEnv } from "@forklaunch/core/environment";

// Load environment with cascading precedence
// Loads all .env.local files from root to current directory
const result = loadCascadingEnv(".env.development", process.cwd());

console.log(result);
// {
//   rootEnvLoaded: true,
//   projectEnvLoaded: true,
//   envFilesLoaded: [
//     '/app/.env.local',
//     '/app/src/modules/my-service/.env.local',
//     '/app/src/modules/my-service/.env.development'
//   ],
//   totalEnvFilesLoaded: 3
// }
```

## Best Practices

### Cache

1. **Use appropriate TTLs** - Short for volatile data, longer for stable data
2. **Handle cache misses** - Always have fallback logic
3. **Use typed keys** - `createCacheKey` ensures consistent naming
4. **Batch operations** - More efficient than individual operations
5. **Monitor cache hit rates** - Optimize based on actual usage

### Object Store

1. **Stream large files** - Don't load entire file into memory
2. **Use hierarchical keys** - Organize with prefixes like `user-files/user-123/avatar.png`
3. **Set proper metadata** - Include contentType and custom metadata
4. **Batch operations** - More efficient for multiple files
5. **Handle errors** - Object store operations can fail

### Testing

1. **Use appropriate timeouts** - Container startup can take 30-60 seconds
2. **Clear database between tests** - Ensure test isolation
3. **Reuse harness** - Setup once for all tests in a suite
4. **Use schema generation** - Faster than migrations for most tests
5. **Use TEST_TOKENS** - Pre-configured for authentication testing

### Utilities

1. **Use type guards** - `isRecord`, `isTrue`, etc. for runtime validation
2. **Cache repeated transformations** - `toCamelCaseIdentifier` results
3. **Use BaseEntity** - Consistent CRUD interface for all entities
4. **Validate identifiers** - Use `isValidIdentifier` before code generation
5. **Use mappers at boundaries** - Controllers yes, services no

## When Claude Code Should Use This Skill

1. **Implementing caching**: Use RedisTtlCache with appropriate patterns
2. **Storing files**: Use S3ObjectStore with streaming for large files
3. **Writing integration tests**: Use BlueprintTestHarness with TestContainers
4. **String manipulation**: Use utility functions instead of manual regex
5. **Entity CRUD**: Use BaseEntity static methods
6. **Data transformation**: Use mappers with validation
7. **Environment setup**: Use loadCascadingEnv for monorepo projects

## Important Notes

- Cache is for small, temporary data; Object Store is for large, permanent files
- Always clean up TestContainers after tests to avoid memory leaks
- Use BaseEntity with EntityManager for proper ORM integration
- Mappers belong in controllers, not services
- All utilities are fully typed with TypeScript
