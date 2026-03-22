import { getEnvVar } from '@forklaunch/common';
import {
  BlueprintTestHarness,
  clearTestDatabase,
  {{#is_database_enabled}}DatabaseType,
  {{/is_database_enabled}}TEST_TOKENS,
  TestSetupResult
} from '@forklaunch/testing';
{{#is_database_enabled}}import { EntityManager, MikroORM } from '@mikro-orm/core';
{{/is_database_enabled}}import dotenv from 'dotenv';
import * as path from 'path';

export { TEST_TOKENS, TestSetupResult };

let harness: BlueprintTestHarness;

dotenv.config({ path: path.join(__dirname, '../.env.test') });

export const setupTestDatabase = async (): Promise<TestSetupResult> => {
  harness = new BlueprintTestHarness({
    {{#is_database_enabled}}getConfig: async () => {
      const { default: config } = await import('../mikro-orm.config');
      return config;
    },
    databaseType: getEnvVar('DATABASE_TYPE') as DatabaseType,
    useMigrations: false,
    {{/is_database_enabled}}needsRedis: {{#is_cache_enabled}}true{{/is_cache_enabled}}{{^is_cache_enabled}}false{{/is_cache_enabled}},
    {{#is_kafka_enabled}}needsKafka: true,
    {{/is_kafka_enabled}}{{#is_s3_enabled}}needsS3: true,
    s3Bucket: 'test-bucket',
    {{/is_s3_enabled}}customEnvVars: {
      PROTOCOL: 'http',
      HOST: 'localhost',
      PORT: '3000',
      VERSION: 'v1',
      DOCS_PATH: '/docs',
      OTEL_SERVICE_NAME: 'test-worker',
      OTEL_LEVEL: 'info',
      OTEL_EXPORTER_OTLP_ENDPOINT: 'http://localhost:4318',
      QUEUE_NAME: 'test-queue'
    }
  });

  return await harness.setup();
};

export const cleanupTestDatabase = async (): Promise<void> => {
  if (harness) {
    await harness.cleanup();
  }
};

export const clearDatabase = async (options?: {
  {{#is_database_enabled}}orm?: MikroORM;
  {{/is_database_enabled}}redis?: TestSetupResult['redis'];
}): Promise<void> => {
  await clearTestDatabase(options);
};
{{#is_database_enabled}}

export const setupTestData = async (em: EntityManager) => {
  const { {{pascal_case_name}}EventRecord } = await import(
    '../persistence/entities/{{camel_case_name}}EventRecord.entity'
  );

  em.create({{pascal_case_name}}EventRecord, {
    id: '123e4567-e89b-12d3-a456-426614174000',
    message: 'Test message',
    processed: false,
    retryCount: 0,
    createdAt: new Date(),
    updatedAt: new Date()
  });

  await em.flush();
};
{{/is_database_enabled}}

export const mock{{pascal_case_name}}Data = {
  message: 'New test message'
};

