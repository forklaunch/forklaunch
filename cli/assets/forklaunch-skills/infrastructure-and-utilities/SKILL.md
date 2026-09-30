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

## Object Store (S3): files, uploads from the browser, download links

### Add it with the CLI, never by hand

```bash
forklaunch infra add <service> object-store     # edits in place: only the lines S3 needs
forklaunch infra remove <service> object-store  # undoes everything add wrote
forklaunch score --offline                      # then confirm the wiring checks pass
```

`infra add` writes the `ObjectStore` registration into `registrations.ts`, MinIO
into docker-compose, local settings into `.env.local`, the
`@forklaunch/infrastructure-s3` dependency, the manifest resource
(`object_store = "s3"`) and the test utilities. Hand-written wiring drifts from
what the platform provisions, and the report card flags it
(`object-store-wiring`).

### How it is configured

The generated registration is keyless by default:

```typescript
new S3ObjectStore(OtelCollector, {
  bucket: S3_BUCKET,
  prefix: S3_PREFIX,                                  // optional; confines every key
  clientConfig: s3ClientConfig({ url: S3_URL, region: S3_REGION,
    accessKeyId: S3_ACCESS_KEY_ID, secretAccessKey: S3_SECRET_ACCESS_KEY }),
  presignLimits: { maxUploadSeconds: S3_PRESIGN_MAX_UPLOAD_SECONDS,
    maxDownloadSeconds: S3_PRESIGN_MAX_DOWNLOAD_SECONDS }
}, telemetry, { encryptor })
```

- **Locally:** `S3_URL` and the `minioadmin` keys point at MinIO.
- **Deployed on ForkLaunch:** the platform sets `S3_BUCKET`, `S3_REGION` and the link caps. It sets no keys; the service's task role reaches its own bucket. `s3ClientConfig` passes keys only when both are set, so keys never have to exist in production.
- **Bucket creation:** the store creates the bucket on first write only when a custom endpoint (MinIO) is set. Deployed buckets are created by the platform.
- **What the platform provisions on AWS:** one private bucket per application, environment and region, so each managed instance gets its own. It has Block Public Access, ACLs disabled, SSE-KMS with its own rotating key, versioning, TLS-only access and a 7-day cleanup of unfinished uploads. Only the services that declare the store get read, write and delete access, through their task role. A reset empties the bucket, and a full destroy deletes it.
- **Settings** live in the object-store resource's `config`: `browserUploads` (default on; off removes CORS), `presignUploadSeconds` (900), `presignDownloadSeconds` (300) and `extraCorsOrigins` (comma-separated, https only). CORS always allows the app's own domains, never `*`. `fl infra config-set` does not accept these for an object store yet.

### Store and read files

```typescript
// A real file: bytes, text or a stream (streams upload in parts).
await objectStore.putFile(`intake/${formId}.pdf`, pdfBuffer, {
  contentType: 'application/pdf',
  filename: 'intake.pdf'          // optional Content-Disposition name
});
const stream = await objectStore.streamDownloadObject(`intake/${formId}.pdf`);

// JSON records (encrypted per tenant when a compliance context is passed).
await objectStore.putObject({ key: `settings/${orgId}`, theme: 'dark' }, { tenantId: orgId });
const settings = await objectStore.readObject<Settings>(`settings/${orgId}`, { tenantId: orgId });
```

`putObject` JSON-encodes its argument; use `putFile` for anything that is a file.

### Browser uploads and download links

Files a user uploads should go straight from the browser to storage, not
through the service. Hand the browser a short-lived grant:

```typescript
// Server: a presigned POST. S3 enforces the size limit and the content type.
const grant = await objectStore.presignUpload(`intake/${formId}.pdf`, {
  contentType: 'application/pdf',
  maxBytes: 20_000_000
});
return grant; // { url, fields, key, expiresAt }

// Browser: post the fields, then the file last.
const form = new FormData();
Object.entries(grant.fields).forEach(([k, v]) => form.append(k, v));
form.append('file', file);
await fetch(grant.url, { method: 'POST', body: form });

// Server: a link to one file, valid for minutes.
const url = await objectStore.presignDownload(`intake/${formId}.pdf`, { filename: 'intake.pdf' });
```

Rules:
- **Link lifetimes are capped by the platform** (defaults: 900 seconds for uploads, 300 for downloads). Asking for longer throws.
- **Never use `getSignedUrl(PutObjectCommand)` for uploads.** A presigned PUT can't limit size (`presigned-upload-unbounded`).
- **CORS belongs to the platform.** It allows only the app's own domains. Don't call `PutBucketCorsCommand`, bucket policies or ACLs from app code (`object-store-bucket-managed-in-app`).
- **Keep files private.** Never set `public-read` (`object-store-public-access`, critical); share files through `presignDownload` links.
- **Encryption:** uploaded files are encrypted by the bucket's KMS key, not by the app. For health data, store the file and keep what it contains classified in the entity that references it.

### Keys

Use `createObjectStoreKey('documents')(id)` for consistent key names. The
store's `prefix` is added for you: keys you pass are relative to it.

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
6. **A misconfigured `REDIS_URL` does not fail fast** - the redis client's reconnect/retry behavior means any `await` touching a Redis-backed cache (role/permission surfacing, session lookups, BullMQ producers/consumers) hangs far longer than any reasonable request timeout, which is easily misdiagnosed as a hung framework call rather than a connectivity problem. If you've remapped Docker Compose host ports for parallel local testing, verify every service's port actually got remapped — `docker ps` takes a few seconds and can save a long debugging detour chasing a "hung" endpoint that's really just talking to a dead port.

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
