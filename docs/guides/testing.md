---
title: Testing Module
category: Framework
description: Complete guide to using the ForkLaunch testing utilities for integration and E2E testing with testcontainers.
---

## Overview

`@forklaunch/testing` provides a comprehensive testing toolkit for ForkLaunch blueprints and applications. It handles the complexity of setting up test infrastructure including databases, Redis, Kafka, S3, and more using Docker containers. The module provides:

- **TestContainerManager**: Docker container orchestration for test databases and services
- **BlueprintTestHarness**: Complete test setup for blueprint integration testing
- **Database Configuration**: Type-safe configs for PostgreSQL, MySQL, MongoDB, MSSQL, SQLite
- **Test Environment Setup**: Automatic environment variable configuration
- **Mock Authentication**: Pre-configured test tokens for authentication testing
- **ORM Integration**: Seamless MikroORM setup with migrations or schema generation

## Quick Start

### Installation

```bash
pnpm add -D @forklaunch/testing testcontainers
```

### Basic Integration Test

```typescript
import { BlueprintTestHarness, TEST_TOKENS } from '@forklaunch/testing';

describe('User API Tests', () => {
  let harness: BlueprintTestHarness;
  let setup: TestSetupResult;

  beforeAll(async () => {
    harness = new BlueprintTestHarness({
      getConfig: async () => {
        const { default: config } = await import('../mikro-orm.config');
        return config;
      },
      databaseType: 'postgres',
      useMigrations: false
    });

    setup = await harness.setup();
  }, 60000);

  afterAll(async () => {
    await harness.cleanup();
  }, 30000);

  it('should create a user', async () => {
    const { createUserRoute } = await import('../api/routes/user.routes');

    const response = await createUserRoute.sdk.createUser({
      body: {
        email: 'test@example.com',
        name: 'Test User'
      },
      headers: {
        authorization: TEST_TOKENS.HMAC
      }
    });

    expect(response.code).toBe(201);
  });
});
```

## Core Concepts

### TestContainerManager

The `TestContainerManager` class manages Docker containers for testing. It supports multiple database types and services, handling container lifecycle, port mapping, and configuration.

**Supported Databases:**
- PostgreSQL
- MySQL/MariaDB
- MongoDB
- Microsoft SQL Server
- SQLite (in-memory, no container needed)

**Supported Services:**
- Redis (caching and queues)
- Kafka (message streaming)
- MinIO/S3 (object storage)

### BlueprintTestHarness

The `BlueprintTestHarness` class provides a complete test environment setup in a single configuration. It orchestrates containers, database initialization, and environment setup, making it ideal for blueprint integration tests.

### Test Lifecycle

1. **Setup Phase**: Start containers, configure environment variables, initialize database
2. **Test Phase**: Run your tests with full infrastructure
3. **Cleanup Phase**: Stop containers, close connections, clean up resources

## API Reference

### TestContainerManager

#### Constructor

```typescript
const manager = new TestContainerManager();
```

#### setupDatabaseContainer

Setup a database container based on type.

```typescript
async setupDatabaseContainer(
  type: DatabaseType,
  config?: DatabaseConfig
): Promise<StartedTestContainer | null>
```

**Parameters:**
- `type`: Database type (`'postgres'`, `'mysql'`, `'mongodb'`, `'mssql'`, `'sqlite'`, etc.)
- `config`: Database-specific configuration (optional)

**Returns:** Started container instance or `null` for SQLite

**Example:**
```typescript
const manager = new TestContainerManager();
const container = await manager.setupDatabaseContainer('postgres', {
  user: 'test_user',
  password: 'test_password',
  database: 'test_db'
});

const host = container.getHost();
const port = container.getMappedPort(5432);
```

#### setupPostgresContainer

Setup a PostgreSQL test container.

```typescript
async setupPostgresContainer(
  config?: PostgresConfig
): Promise<StartedTestContainer>
```

**PostgresConfig:**
```typescript
interface PostgresConfig {
  user?: string;              // Default: 'test_user'
  password?: string;          // Default: 'test_password'
  database?: string;          // Default: 'test_db'
  command?: string[];         // Default: ['postgres', '-c', 'log_statement=all']
}
```

**Example:**
```typescript
const container = await manager.setupPostgresContainer({
  user: 'myuser',
  password: 'mypass',
  database: 'mydb',
  command: ['postgres', '-c', 'max_connections=200']
});
```

#### setupMySQLContainer

Setup a MySQL test container.

```typescript
async setupMySQLContainer(
  config?: MySQLConfig
): Promise<StartedTestContainer>
```

**MySQLConfig:**
```typescript
interface MySQLConfig {
  user?: string;              // Default: 'test_user'
  password?: string;          // Default: 'test_password'
  database?: string;          // Default: 'test_db'
  rootPassword?: string;      // Default: 'root_password'
}
```

**Example:**
```typescript
const container = await manager.setupMySQLContainer({
  user: 'myuser',
  password: 'mypass',
  database: 'mydb',
  rootPassword: 'rootpass'
});
```

#### setupMongoDBContainer

Setup a MongoDB test container.

```typescript
async setupMongoDBContainer(
  config?: MongoDBConfig
): Promise<StartedTestContainer>
```

**MongoDBConfig:**
```typescript
interface MongoDBConfig {
  user?: string;              // Default: 'test_user'
  password?: string;          // Default: 'test_password'
  database?: string;          // Default: 'test_db'
}
```

**Example:**
```typescript
const container = await manager.setupMongoDBContainer({
  user: 'admin',
  password: 'secret',
  database: 'testdb'
});
```

#### setupMSSQLContainer

Setup a Microsoft SQL Server test container.

```typescript
async setupMSSQLContainer(
  config?: MSSQLConfig
): Promise<StartedTestContainer>
```

**MSSQLConfig:**
```typescript
interface MSSQLConfig {
  user?: string;              // Default: 'SA'
  password?: string;          // Default: 'Test_Password123!'
  database?: string;          // Default: 'test_db'
  saPassword?: string;        // Default: 'Test_Password123!'
}
```

**Example:**
```typescript
const container = await manager.setupMSSQLContainer({
  saPassword: 'MyStrong!Pass123'
});
```

#### setupRedisContainer

Setup a Redis test container.

```typescript
async setupRedisContainer(
  config?: RedisConfig
): Promise<StartedTestContainer>
```

**RedisConfig:**
```typescript
interface RedisConfig {
  command?: string[];         // Default: ['redis-server', '--appendonly', 'yes']
}
```

**Example:**
```typescript
const container = await manager.setupRedisContainer({
  command: ['redis-server', '--maxmemory', '256mb']
});
```

#### setupKafkaContainer

Setup a Kafka test container.

```typescript
async setupKafkaContainer(
  config?: KafkaConfig
): Promise<StartedTestContainer>
```

**KafkaConfig:**
```typescript
interface KafkaConfig {
  clusterId?: string;              // Default: 'test-cluster'
  numPartitions?: number;          // Default: 1
  replicationFactor?: number;      // Default: 1
  env?: Record<string, string>;    // Additional environment variables
}
```

**Example:**
```typescript
const container = await manager.setupKafkaContainer({
  clusterId: 'my-cluster',
  numPartitions: 3,
  replicationFactor: 1
});

const broker = `${container.getHost()}:${container.getMappedPort(9092)}`;
```

#### setupS3Container

Setup a MinIO (S3-compatible) test container.

```typescript
async setupS3Container(
  config?: S3Config
): Promise<StartedTestContainer>
```

**S3Config:**
```typescript
interface S3Config {
  rootUser?: string;           // Default: 'minioadmin'
  rootPassword?: string;       // Default: 'minioadmin'
  defaultBucket?: string;      // Default: 'test-bucket'
  region?: string;             // Default: 'us-east-1'
}
```

**Example:**
```typescript
const container = await manager.setupS3Container({
  rootUser: 'myaccesskey',
  rootPassword: 'mysecretkey',
  defaultBucket: 'my-test-bucket',
  region: 'us-west-2'
});

const endpoint = `http://${container.getHost()}:${container.getMappedPort(9000)}`;
```

#### cleanup

Stop and remove all managed containers.

```typescript
async cleanup(): Promise<void>
```

**Example:**
```typescript
await manager.cleanup(); // Stops all containers and removes volumes
```

### BlueprintTestHarness

#### Constructor

```typescript
const harness = new BlueprintTestHarness(config: BlueprintTestConfig);
```

**BlueprintTestConfig:**
```typescript
interface BlueprintTestConfig {
  /**
   * Function that imports and returns the MikroORM config
   * Called AFTER environment variables are set
   * Optional - if not provided, no database will be set up
   */
  getConfig?: () => Promise<Options>;

  /**
   * Database type (postgres, mysql, mongodb, etc.)
   * Optional - if not provided, no database will be set up
   */
  databaseType?: DatabaseType;

  /**
   * Whether to use migrations (true) or schema generation (false)
   */
  useMigrations?: boolean;

  /**
   * Path to migrations directory (required if useMigrations is true)
   */
  migrationsPath?: string;

  /**
   * Whether the blueprint needs Redis
   */
  needsRedis?: boolean;

  /**
   * Whether the blueprint needs Kafka
   */
  needsKafka?: boolean;

  /**
   * Whether the blueprint needs S3 (MinIO)
   */
  needsS3?: boolean;

  /**
   * S3 bucket name to create (default: 'test-bucket')
   */
  s3Bucket?: string;

  /**
   * Custom environment variables to set
   */
  customEnvVars?: Record<string, string>;

  /**
   * Custom setup hook called after containers and ORM are initialized
   */
  onSetup?: (setup: TestSetupResult) => Promise<void>;
}
```

#### setup

Initialize all test infrastructure.

```typescript
async setup(): Promise<TestSetupResult>
```

**TestSetupResult:**
```typescript
interface TestSetupResult {
  container: StartedTestContainer | null;
  redisContainer?: StartedTestContainer;
  kafkaContainer?: StartedTestContainer;
  s3Container?: StartedTestContainer;
  orm?: MikroORM;
  redis?: Redis;
}
```

**Example:**
```typescript
const harness = new BlueprintTestHarness({
  getConfig: async () => {
    const { default: config } = await import('../mikro-orm.config');
    return config;
  },
  databaseType: 'postgres',
  needsRedis: true,
  customEnvVars: {
    STRIPE_API_KEY: 'sk_test_...'
  }
});

const setup = await harness.setup();
// Use setup.orm, setup.redis, etc.
```

#### cleanup

Stop all containers and close all connections.

```typescript
async cleanup(): Promise<void>
```

**Example:**
```typescript
await harness.cleanup();
```

#### clearDatabase

Clear all data from database and cache.

```typescript
async clearDatabase(): Promise<void>
```

**Example:**
```typescript
beforeEach(async () => {
  await harness.clearDatabase(); // Clean slate for each test
});
```

### Database Utilities

#### setupTestORM

Setup MikroORM for testing with proper schema/migrations.

```typescript
async function setupTestORM(
  config: MikroOrmTestConfig
): Promise<MikroORM>
```

**MikroOrmTestConfig:**
```typescript
interface MikroOrmTestConfig {
  mikroOrmConfig: Options;           // MikroORM config object
  databaseType: DatabaseType;        // Database type
  useMigrations?: boolean;           // true: use migrations, false: schema generation
  migrationsPath?: string;           // Path to migrations (required if useMigrations is true)
  container: StartedTestContainer | null; // Container instance (null for SQLite)
}
```

**Example:**
```typescript
import { setupTestORM } from '@forklaunch/testing';

const orm = await setupTestORM({
  mikroOrmConfig: config,
  databaseType: 'postgres',
  useMigrations: false,
  container: postgresContainer
});
```

#### clearTestDatabase

Clear all data from the test database and/or cache.

```typescript
async function clearTestDatabase(options?: {
  orm?: MikroORM;
  redis?: Redis;
}): Promise<void>
```

**Example:**
```typescript
import { clearTestDatabase } from '@forklaunch/testing';

await clearTestDatabase({
  orm: setup.orm,
  redis: setup.redis
});
```

### Environment Setup

#### setupTestEnvironment

Configure environment variables for testing.

```typescript
function setupTestEnvironment(config: TestEnvConfig): void
```

**TestEnvConfig:**
```typescript
interface TestEnvConfig {
  database: StartedTestContainer | null;
  databaseType?: DatabaseType;
  redis?: StartedTestContainer;
  kafka?: StartedTestContainer;
  s3?: StartedTestContainer;
  hmacSecret?: string;              // Default: 'test-secret-key'
  customVars?: Record<string, string>;
}
```

**Environment Variables Set:**
- Database: `DB_HOST`, `DB_PORT`, `DB_USER`, `DB_PASSWORD`, `DB_NAME`
- Redis: `REDIS_URL`, `REDIS_HOST`, `REDIS_PORT`
- Kafka: `KAFKA_BROKERS`, `KAFKA_CLIENT_ID`, `KAFKA_GROUP_ID`
- S3: `S3_ENDPOINT`, `S3_ACCESS_KEY_ID`, `S3_SECRET_ACCESS_KEY`, `S3_REGION`, `S3_BUCKET`
- Standard: `HMAC_SECRET_KEY`, `NODE_ENV`, `HOST`, `PORT`, etc.

**Example:**
```typescript
import { setupTestEnvironment } from '@forklaunch/testing';

setupTestEnvironment({
  database: postgresContainer,
  databaseType: 'postgres',
  redis: redisContainer,
  customVars: {
    API_KEY: 'test-key',
    STRIPE_API_KEY: 'sk_test_...'
  }
});

// Now process.env.DB_HOST, process.env.REDIS_URL, etc. are set
```

### Test Tokens

Pre-configured authentication tokens for testing.

```typescript
const TEST_TOKENS = {
  /**
   * Mock Bearer token for testing
   */
  AUTH: 'Bearer test-token',

  /**
   * Mock valid HMAC token for testing
   */
  HMAC: 'HMAC keyId=test-key ts=1234567890 nonce=test-nonce signature=test-signature',

  /**
   * Mock invalid HMAC token for testing authentication failures
   */
  HMAC_INVALID: 'HMAC keyId=invalid-key ts=1234567890 nonce=invalid-nonce signature=invalid-signature'
} as const;
```

**Example:**
```typescript
import { TEST_TOKENS } from '@forklaunch/testing';

// Test authenticated endpoint
const response = await createUserRoute.sdk.createUser({
  body: userData,
  headers: {
    authorization: TEST_TOKENS.HMAC
  }
});

// Test authentication failure
await expect(async () => {
  await createUserRoute.sdk.createUser({
    body: userData,
    headers: {
      authorization: TEST_TOKENS.HMAC_INVALID
    }
  });
}).rejects.toThrow();
```

## Common Patterns

### Blueprint Testing with Database

```typescript
import { BlueprintTestHarness, TEST_TOKENS } from '@forklaunch/testing';
import dotenv from 'dotenv';
import * as path from 'path';

let harness: BlueprintTestHarness;

dotenv.config({ path: path.join(__dirname, '../.env.test') });

export const setupTestDatabase = async () => {
  harness = new BlueprintTestHarness({
    getConfig: async () => {
      const { default: config } = await import('../mikro-orm.config');
      return config;
    },
    databaseType: 'postgres',
    useMigrations: false, // Use schema generation for faster tests
    needsRedis: true,
    customEnvVars: {
      STRIPE_API_KEY: process.env.STRIPE_API_KEY!
    }
  });

  return await harness.setup();
};

export const cleanupTestDatabase = async () => {
  if (harness) {
    await harness.cleanup();
  }
};

describe('Subscription API', () => {
  beforeAll(async () => {
    await setupTestDatabase();
  }, 60000);

  afterAll(async () => {
    await cleanupTestDatabase();
  }, 30000);

  it('should create a subscription', async () => {
    const { createSubscriptionRoute } = await import('../api/routes/subscription.routes');

    const response = await createSubscriptionRoute.sdk.createSubscription({
      body: {
        partyId: 'user-123',
        partyType: 'USER',
        productId: 'prod-456',
        billingProvider: 'STRIPE'
      },
      headers: {
        authorization: TEST_TOKENS.HMAC
      }
    });

    expect(response.code).toBe(200);
    expect(response.response.id).toBeDefined();
  });
});
```

### Blueprint Testing with Migrations

```typescript
import { BlueprintTestHarness, clearTestDatabase } from '@forklaunch/testing';
import * as path from 'path';

let harness: BlueprintTestHarness;
let orm: MikroORM;

const setupTestDatabase = async () => {
  harness = new BlueprintTestHarness({
    getConfig: async () => {
      const { default: config } = await import('../mikro-orm.config');
      return config;
    },
    databaseType: 'postgres',
    useMigrations: true, // Use migrations for IAM blueprints
    migrationsPath: path.join(__dirname, '../migrations')
  });

  const setup = await harness.setup();
  orm = setup.orm!;
  return setup;
};

const setupTestData = async (em: EntityManager) => {
  const { User } = await import('../persistence/entities/user.entity');
  const { Role } = await import('../persistence/entities/role.entity');

  const role = em.create(Role, {
    id: '123e4567-e89b-12d3-a456-426614174000',
    name: 'admin'
  });

  em.create(User, {
    id: '123e4567-e89b-12d3-a456-426614174001',
    email: 'test@example.com',
    name: 'Test User',
    roles: [role]
  });

  await em.flush();
};

describe('User API', () => {
  beforeAll(async () => {
    await setupTestDatabase();
  }, 60000);

  beforeEach(async () => {
    await clearTestDatabase({ orm });
    const em = orm.em.fork();
    await setupTestData(em);
  });

  afterAll(async () => {
    await harness.cleanup();
  }, 30000);

  it('should get a user', async () => {
    const { getUserRoute } = await import('../api/routes/user.routes');

    const response = await getUserRoute.sdk.getUser({
      params: { id: '123e4567-e89b-12d3-a456-426614174001' },
      headers: {
        authorization: TEST_TOKENS.AUTH
      }
    });

    expect(response.code).toBe(200);
    expect(response.response.email).toBe('test@example.com');
  });
});
```

### Cache-Only Testing (No Database)

```typescript
import { BlueprintTestHarness } from '@forklaunch/testing';

const harness = new BlueprintTestHarness({
  needsRedis: true,
  customEnvVars: {
    API_KEY: 'test-key'
  }
});

const setup = await harness.setup();
// setup.orm is undefined
// setup.redis is available

// Test cache operations
await setup.redis!.set('test-key', 'test-value');
const value = await setup.redis!.get('test-key');
expect(value).toBe('test-value');

await harness.cleanup();
```

### Testing with Multiple Services

```typescript
import { BlueprintTestHarness } from '@forklaunch/testing';

const harness = new BlueprintTestHarness({
  getConfig: async () => {
    const { default: config } = await import('../mikro-orm.config');
    return config;
  },
  databaseType: 'postgres',
  needsRedis: true,
  needsKafka: true,
  needsS3: true,
  s3Bucket: 'test-uploads',
  customEnvVars: {
    API_KEY: 'test-key'
  },
  onSetup: async (setup) => {
    // Custom setup logic after infrastructure is ready
    console.log('Kafka broker:', process.env.KAFKA_BROKERS);
    console.log('S3 endpoint:', process.env.S3_ENDPOINT);

    // Seed initial data
    const em = setup.orm!.em.fork();
    // ... create initial entities
    await em.flush();
  }
});

const setup = await harness.setup();

// All services available:
// - setup.orm (PostgreSQL)
// - setup.redis
// - setup.kafkaContainer
// - setup.s3Container

await harness.cleanup();
```

### Low-Level Container Management

```typescript
import { TestContainerManager } from '@forklaunch/testing';

const manager = new TestContainerManager();

// Start multiple services
const postgres = await manager.setupPostgresContainer();
const redis = await manager.setupRedisContainer();
const kafka = await manager.setupKafkaContainer({
  numPartitions: 5
});

// Use containers
const pgHost = postgres.getHost();
const pgPort = postgres.getMappedPort(5432);
const redisHost = redis.getHost();
const redisPort = redis.getMappedPort(6379);

// Cleanup all
await manager.cleanup();
```

### Custom Environment Setup

```typescript
import {
  TestContainerManager,
  setupTestEnvironment,
  setupTestORM
} from '@forklaunch/testing';

const manager = new TestContainerManager();
const postgres = await manager.setupPostgresContainer();
const redis = await manager.setupRedisContainer();

// Setup environment
setupTestEnvironment({
  database: postgres,
  databaseType: 'postgres',
  redis: redis,
  customVars: {
    MY_CUSTOM_VAR: 'custom-value',
    STRIPE_API_KEY: 'sk_test_...'
  }
});

// Setup ORM manually
const config = await import('../mikro-orm.config');
const orm = await setupTestORM({
  mikroOrmConfig: config.default,
  databaseType: 'postgres',
  useMigrations: false,
  container: postgres
});

// Run tests
// ...

// Cleanup
await orm.close();
await manager.cleanup();
```

## Testing Best Practices

### 1. Use Appropriate Timeouts

Container startup can be slow - set generous timeouts:

```typescript
beforeAll(async () => {
  await setupTestDatabase();
}, 60000); // 60 second timeout

afterAll(async () => {
  await cleanupTestDatabase();
}, 30000); // 30 second timeout
```

### 2. Clean Database Between Tests

```typescript
beforeEach(async () => {
  await harness.clearDatabase(); // Fresh state for each test
});
```

### 3. Use Schema Generation for Speed

```typescript
// ✅ Fast - for most blueprints
useMigrations: false

// ⚠️ Slower but required for IAM blueprints with custom migrations
useMigrations: true
```

### 4. Reuse Harness Across Tests

```typescript
// ✅ Good - one setup for all tests
let harness: BlueprintTestHarness;

beforeAll(async () => {
  harness = new BlueprintTestHarness({ ... });
  await harness.setup();
});

afterAll(async () => {
  await harness.cleanup();
});

// ❌ Bad - setup/teardown for each test
beforeEach(async () => {
  harness = new BlueprintTestHarness({ ... });
  await harness.setup(); // Too slow!
});

afterEach(async () => {
  await harness.cleanup();
});
```

### 5. Use TypeScript for Type Safety

```typescript
// ✅ Type-safe test data
interface CreateUserInput {
  email: string;
  name: string;
  roles: string[];
}

const mockUser: CreateUserInput = {
  email: 'test@example.com',
  name: 'Test User',
  roles: ['admin']
};

const response = await createUserRoute.sdk.createUser({
  body: mockUser,
  headers: { authorization: TEST_TOKENS.HMAC }
});
```

### 6. Handle Async Operations

```typescript
// ✅ Good - await all operations
await harness.setup();
await harness.clearDatabase();
await harness.cleanup();

// ❌ Bad - missing await
harness.setup(); // Returns promise that's not awaited!
```

### 7. Organize Test Utilities

```typescript
// test-utils.ts
export { TEST_TOKENS, TestSetupResult };

let harness: BlueprintTestHarness;

export const setupTestDatabase = async () => {
  harness = new BlueprintTestHarness({ ... });
  return await harness.setup();
};

export const cleanupTestDatabase = async () => {
  if (harness) await harness.cleanup();
};

export const clearDatabase = async (options) => {
  await clearTestDatabase(options);
};

// In tests
import { setupTestDatabase, cleanupTestDatabase } from './test-utils';
```

### 8. Mock External Services

```typescript
const harness = new BlueprintTestHarness({
  // ... other config
  customEnvVars: {
    STRIPE_API_KEY: 'sk_test_...',  // Use test keys
    EXTERNAL_API_URL: 'http://localhost:3001' // Mock server
  },
  onSetup: async (setup) => {
    // Setup mock data in external services
    const stripe = new Stripe(process.env.STRIPE_API_KEY);
    const product = await stripe.products.create({ name: 'Test Product' });
    process.env.TEST_PRODUCT_ID = product.id;
  }
});
```

## Performance Optimization

### Container Reuse

TestContainers can be slow to start. Consider:

1. **Use schema generation** instead of migrations when possible
2. **Minimize container count** - only start what you need
3. **Share containers** across test suites when safe
4. **Use in-memory SQLite** for unit tests

### Parallel Testing

```typescript
// vitest.config.ts
export default {
  test: {
    pool: 'forks',
    poolOptions: {
      forks: {
        maxForks: 4 // Run 4 test files in parallel
      }
    }
  }
};
```

### Cleanup Strategies

```typescript
// Fast cleanup - just truncate tables
beforeEach(async () => {
  await clearTestDatabase({ orm }); // Deletes all entities
});

// Slower but complete - recreate schema
beforeEach(async () => {
  await orm.getSchemaGenerator().dropSchema();
  await orm.getSchemaGenerator().createSchema();
});
```

## Troubleshooting

### Container Won't Start

```typescript
// Check Docker is running
docker ps

// Check port availability
lsof -i :5432

// Increase timeout
beforeAll(async () => {
  await setupTestDatabase();
}, 120000); // 2 minutes
```

### Database Connection Errors

```typescript
// Verify environment variables are set
console.log(process.env.DB_HOST);
console.log(process.env.DB_PORT);

// Check container is healthy
const host = container.getHost();
const port = container.getMappedPort(5432);
console.log(`Database at ${host}:${port}`);
```

### Memory Issues

```typescript
// Cleanup containers properly
afterAll(async () => {
  await harness.cleanup(); // Must call cleanup!
}, 30000);

// Or manually
afterAll(async () => {
  await orm.close();
  await manager.cleanup();
});
```

### Tests Hanging

```typescript
// Ensure all async operations complete
await harness.setup();    // ✅
await harness.cleanup();  // ✅

harness.setup();    // ❌ Missing await
harness.cleanup();  // ❌ Missing await
```

## Related Documentation

- [Cache](/docs/guides/cache.md): Testing with Redis cache
- [Config Injector](/docs/framework/config.md): Environment configuration
- [HTTP Framework](/docs/framework/http.md): Testing HTTP routes
- [Validation](/docs/framework/validation.md): Schema validation in tests
