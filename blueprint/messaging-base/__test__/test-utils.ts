import { getEnvVar } from '@forklaunch/common';
import {
  BlueprintTestHarness,
  clearTestDatabase,
  DatabaseType,
  TEST_TOKENS,
  TestSetupResult
} from '@forklaunch/testing';
import { EntityManager } from '@mikro-orm/core';
import dotenv from 'dotenv';
import Redis from 'ioredis';
import * as path from 'path';

export { TEST_TOKENS, TestSetupResult };

let harness: BlueprintTestHarness;

dotenv.config({ path: path.join(__dirname, '../.env.test') });

export const setupTestDatabase = async (): Promise<TestSetupResult> => {
  harness = new BlueprintTestHarness({
    getConfig: async () => {
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
    useMigrations: false,
    needsRedis: true
  });

  return await harness.setup();
};

export const cleanupTestDatabase = async (): Promise<void> => {
  if (harness) {
    await harness.cleanup();
  }
};

export const clearDatabase = async (options?: {
  orm?: TestSetupResult['orm'];
  redis?: Redis;
}): Promise<void> => {
  await clearTestDatabase(options);
};

export const setupTestData = async (em: EntityManager) => {
  const { SmsRecord } = await import(
    '../persistence/entities/smsRecord.entity'
  );
  const { SmsStatusEnum } = await import(
    '@forklaunch/implementation-messaging-base/enum'
  );

  em.create(SmsRecord, {
    id: '123e4567-e89b-12d3-a456-426614174001',
    to: '+15555550100',
    body: 'A seeded sms record',
    status: SmsStatusEnum.SENT,
    providerMessageId: 'SM_test_123',
    error: null,
    metadata: null
  });

  await em.flush();
};

export const mockSendSmsData = {
  to: '+15555550199',
  body: 'Hello from the test suite',
  metadata: {
    campaign: 'test'
  }
};
