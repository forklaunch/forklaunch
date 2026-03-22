import { MikroORM, Options } from '@mikro-orm/core';
import Redis from 'ioredis';
import { StartedTestContainer } from 'testcontainers';
import { DatabaseType, TestContainerManager } from './containers';
import { clearTestDatabase, setupTestORM } from './database';
import { setupTestEnvironment } from './environment';

export interface BlueprintTestConfig {
  /**
   * Function that imports and returns the MikroORM config
   * This is called AFTER environment variables are set
   * Optional - if not provided, no database will be set up
   */
  getConfig?: () => Promise<Partial<Options>>;

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

export interface TestSetupResult {
  container: StartedTestContainer | null;
  redisContainer?: StartedTestContainer;
  kafkaContainer?: StartedTestContainer;
  s3Container?: StartedTestContainer;
  orm?: MikroORM;
  redis?: Redis;
}

/**
 * Complete test harness for blueprint E2E testing
 *
 * Handles container setup, environment configuration, and database initialization
 *
 * @example Database with ORM
 * ```typescript
 * const harness = new BlueprintTestHarness({
 *   getConfig: async () => {
 *     const { default: config } = await import('../mikro-orm.config');
 *     return config;
 *   },
 *   databaseType: 'postgres',
 *   useMigrations: false,
 *   needsRedis: true,
 *   customEnvVars: {
 *     STRIPE_API_KEY: 'sk_test_...'
 *   }
 * });
 *
 * const setup = await harness.setup();
 * // ... run tests with setup.orm and setup.redis
 * await harness.cleanup();
 * ```
 *
 * @example Cache-only (no database)
 * ```typescript
 * const harness = new BlueprintTestHarness({
 *   needsRedis: true,
 *   customEnvVars: {
 *     API_KEY: 'test_key'
 *   }
 * });
 *
 * const setup = await harness.setup();
 * // ... run tests with setup.redis only (setup.orm is undefined)
 * await harness.cleanup();
 * ```
 *
 * @example With Kafka and S3
 * ```typescript
 * const harness = new BlueprintTestHarness({
 *   getConfig: async () => {
 *     const { default: config } = await import('../mikro-orm.config');
 *     return config;
 *   },
 *   databaseType: 'postgres',
 *   needsKafka: true,
 *   needsS3: true,
 *   s3Bucket: 'my-test-bucket',
 *   customEnvVars: {
 *     API_KEY: 'test_key'
 *   }
 * });
 *
 * const setup = await harness.setup();
 * // Access containers via setup.kafkaContainer and setup.s3Container
 * // Kafka broker: process.env.KAFKA_BROKERS
 * // S3 endpoint: process.env.S3_ENDPOINT
 * await harness.cleanup();
 * ```
 */
export class BlueprintTestHarness {
  private containers: TestContainerManager;
  private result?: TestSetupResult;

  constructor(private config: BlueprintTestConfig) {
    this.containers = new TestContainerManager();
  }

  /**
   * Setup all test infrastructure (containers, ORM, Redis, Kafka, S3)
   */
  async setup(): Promise<TestSetupResult> {
    // Setup database container only if database is needed
    let container: StartedTestContainer | null = null;
    let orm: MikroORM | undefined;
    let redisContainer: StartedTestContainer | undefined;
    let kafkaContainer: StartedTestContainer | undefined;
    let s3Container: StartedTestContainer | undefined;

    // Setup Redis container if needed (for both database and cache-only modes)
    if (this.config.needsRedis) {
      redisContainer = await this.containers.setupRedisContainer();
    }

    // Setup Kafka container if needed
    if (this.config.needsKafka) {
      kafkaContainer = await this.containers.setupKafkaContainer();
    }

    // Setup S3 container if needed
    if (this.config.needsS3) {
      s3Container = await this.containers.setupS3Container({
        defaultBucket: this.config.s3Bucket
      });
    }

    if (this.config.databaseType && this.config.getConfig) {
      const databaseType = this.config.databaseType;

      // Setup database container
      container = await this.containers.setupDatabaseContainer(databaseType);

      // Setup environment variables
      setupTestEnvironment({
        database: container,
        databaseType,
        redis: redisContainer,
        kafka: kafkaContainer,
        s3: s3Container,
        customVars: this.config.customEnvVars
      });

      // Get the config AFTER environment is set
      const mikroOrmConfig = await this.config.getConfig();

      // Setup ORM
      orm = await setupTestORM({
        mikroOrmConfig,
        databaseType,
        useMigrations: this.config.useMigrations,
        migrationsPath: this.config.migrationsPath,
        container
      });
    } else {
      // Cache-only mode: no database
      setupTestEnvironment({
        database: null,
        databaseType: undefined,
        redis: redisContainer,
        kafka: kafkaContainer,
        s3: s3Container,
        customVars: this.config.customEnvVars
      });
    }

    // Setup Redis client if needed
    let redis: Redis | undefined;
    if (redisContainer) {
      redis = new Redis({
        host: redisContainer.getHost(),
        port: redisContainer.getMappedPort(6379),
        maxRetriesPerRequest: 3
      });
      await redis.ping();
    }

    this.result = {
      container,
      redisContainer,
      kafkaContainer,
      s3Container,
      orm,
      redis
    };

    // Call custom setup hook
    if (this.config.onSetup) {
      await this.config.onSetup(this.result);
    }

    return this.result;
  }

  /**
   * Cleanup all test infrastructure
   */
  async cleanup(): Promise<void> {
    if (this.result?.redis) {
      await this.result.redis.quit();
    }
    if (this.result?.orm) {
      await this.result.orm.close();
    }
    await this.containers.cleanup();
    this.result = undefined;
  }

  /**
   * Clear all data from the database and/or cache
   */
  async clearDatabase(): Promise<void> {
    if (this.result) {
      await clearTestDatabase({
        orm: this.result.orm,
        redis: this.result.redis
      });
    }
  }
}
