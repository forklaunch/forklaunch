/**
 * @forklaunch/testing
 *
 * Testing utilities for forklaunch-js blueprints
 *
 * @packageDocumentation
 */

// Container management
export {
  DatabaseConfig,
  DatabaseType,
  KafkaConfig,
  MongoDBConfig,
  MSSQLConfig,
  MySQLConfig,
  PostgresConfig,
  RedisConfig,
  S3Config,
  SQLiteConfig,
  TestContainerManager
} from './containers';

export { setupTestEnvironment, TestEnvConfig } from './environment';

export {
  clearTestDatabase,
  MikroOrmTestConfig,
  setupTestORM,
  type AnyMikroORM,
  type AnyMikroOrmOptions
} from './database';

export {
  BlueprintTestConfig,
  BlueprintTestHarness,
  TestSetupResult
} from './harness';

export { TEST_TOKENS } from './tokens';
