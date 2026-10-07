import { getEnvVar } from '@forklaunch/common';
import {
  {{#is_database_enabled}}AnyMikroORM,
  {{/is_database_enabled}}BlueprintTestHarness,
  clearTestDatabase,
  {{#is_database_enabled}}DatabaseType,
  {{/is_database_enabled}}TEST_TOKENS,
  TestSetupResult
} from '@forklaunch/testing';
{{#is_database_enabled}}import { EntityManager } from '@mikro-orm/core';{{/is_database_enabled}}
import dotenv from 'dotenv';
import * as path from 'path';

export { TEST_TOKENS, TestSetupResult };

let harness: BlueprintTestHarness;

dotenv.config({ path: path.join(__dirname, '../.env.test') });

export const setupTestDatabase = async (): Promise<TestSetupResult> => {
  harness = new BlueprintTestHarness({
    {{#is_database_enabled}}getConfig: async () => {
      const { default: config } = await import('../mikro-orm.config');
      // MikroORM.init() mutates options.discovery.skipSyncDiscovery = true on
      // the object it receives. mikro-orm.config exports a single shared object
      // that the app's own DI container also builds a MikroORM from, so letting
      // the harness mutate it leaves the app's `new MikroORM(config)` with an
      // undefined `.em` (every route then crashes on `Orm.em.fork`). Hand the
      // harness its own discovery object so the mutation can't leak.
      return { ...config, discovery: { ...config.discovery } };
    },
    databaseType: getEnvVar('DATABASE_TYPE') as DatabaseType,
    useMigrations: true,
    migrationsPath: path.join(__dirname, getEnvVar("MIGRATIONS_PATH")),
    {{/is_database_enabled}}needsRedis: {{#is_cache_enabled}}true{{/is_cache_enabled}}{{^is_cache_enabled}}false{{/is_cache_enabled}},
    {{#is_kafka_enabled}}needsKafka: true,
    {{/is_kafka_enabled}}{{#is_s3_enabled}}needsS3: true,
    s3Bucket: 'test-bucket',
    {{/is_s3_enabled}}
  });

  return await harness.setup();
};

export const cleanupTestDatabase = async (): Promise<void> => {
  if (harness) {
    await harness.cleanup();
  }
};

export const clearDatabase = async (options?: {
  {{#is_database_enabled}}orm?: AnyMikroORM;
  {{/is_database_enabled}}redis?: TestSetupResult['redis'];
}): Promise<void> => {
  await clearTestDatabase(options);
};
{{#is_database_enabled}}

export const setupTestData = async (em: EntityManager) => {
  const { {{pascal_case_name}}Record } = await import(
    '../persistence/entities/{{camel_case_name}}Record.entity'
  );

  em.create({{pascal_case_name}}Record, {
    id: '123e4567-e89b-12d3-a456-426614174000',
    message: 'Test message'
  });

  await em.flush();
};
{{/is_database_enabled}}

export const mock{{pascal_case_name}}Data = {
  message: 'New test message'
};
