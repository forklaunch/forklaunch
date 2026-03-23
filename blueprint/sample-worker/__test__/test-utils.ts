import {
  BlueprintTestHarness,
  clearTestDatabase,
  TEST_TOKENS,
  TestSetupResult
} from '@forklaunch/testing';
import { EntityManager, MikroORM } from '@mikro-orm/core';

export { TEST_TOKENS, TestSetupResult };

let harness: BlueprintTestHarness;

export const setupTestDatabase = async (): Promise<TestSetupResult> => {
  harness = new BlueprintTestHarness({
    getConfig: async () => {
      const { default: config } = await import('../mikro-orm.config');
      return config;
    },
    databaseType: 'postgres',
    useMigrations: false,
    needsRedis: true,
    customEnvVars: {
      PROTOCOL: 'http',
      KAFKA_BROKERS: 'localhost:9092',
      KAFKA_CLIENT_ID: 'test-client',
      KAFKA_GROUP_ID: 'test-group',
      SAMPLE_WORKER_QUEUE: 'test-queue',
      S3_REGION: 'us-east-1',
      S3_ACCESS_KEY_ID: 'test-access-key',
      S3_SECRET_ACCESS_KEY: 'test-secret-key',
      S3_BUCKET: 'test-bucket',
      S3_URL: 'http://localhost:9000'
    }
  });

  return await harness.setup();
};

export const cleanupTestDatabase = async (): Promise<void> => {
  if (harness) {
    await harness.cleanup();
  }
};

export async function clearDatabase(options?: {
  orm?: MikroORM;
  redis?: TestSetupResult['redis'];
}): Promise<void> {
  await clearTestDatabase(options);
}

export const setupTestData = async (em: EntityManager) => {
  const { SampleWorkerEventRecord } = await import(
    '../persistence/entities/sampleWorkerRecord.entity'
  );

  em.create(SampleWorkerEventRecord, {
    id: '123e4567-e89b-12d3-a456-426614174000',
    message: 'Test message',
    processed: false,
    retryCount: 0
  });

  await em.flush();
};

export const mockSampleWorkerData = {
  message: 'New test message'
};
