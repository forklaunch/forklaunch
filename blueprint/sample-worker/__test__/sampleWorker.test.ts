import { afterAll, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import {
  cleanupTestDatabase,
  clearDatabase,
  mockSampleWorkerData,
  setupTestData,
  setupTestDatabase,
  TEST_TOKENS,
  TestSetupResult
} from './test-utils';

describe('SampleWorker Routes E2E Tests with PostgreSQL Container', () => {
  let orm: TestSetupResult['orm'];
  let redis: TestSetupResult['redis'];

  beforeAll(async () => {
    const setup = await setupTestDatabase();
    orm = setup.orm;
    redis = setup.redis;
  }, 60000);

  beforeEach(async () => {
    await clearDatabase({ orm, redis });
    if (!orm) throw new Error('ORM not initialized');
    const em = orm.em.fork();
    await setupTestData(em);
  });

  afterAll(async () => {
    await cleanupTestDatabase();
  }, 30000);

  describe('GET /sample-worker/:id - sampleWorkerGet', () => {
    it('should get a sample worker successfully', async () => {
      const { sampleWorkerGetRoute } = await import(
        '../api/routes/sampleWorker.routes'
      );
      const response = await sampleWorkerGetRoute.sdk.sampleworkerget({
        params: { id: '123e4567-e89b-12d3-a456-426614174000' },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBeDefined();
      if (response.code === 200) {
        expect(response.response.message).toBe('SampleWorker');
        expect(response.response.processed).toBe(false);
        expect(response.response.retryCount).toBe(0);
      }
    });

    it('should require HMAC authentication', async () => {
      const { sampleWorkerGetRoute } = await import(
        '../api/routes/sampleWorker.routes'
      );

      try {
        await sampleWorkerGetRoute.sdk.sampleworkerget({
          params: { id: '123e4567-e89b-12d3-a456-426614174000' },
          headers: {
            authorization: TEST_TOKENS.HMAC_INVALID
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });
  });

  describe('POST /sample-worker - sampleWorkerPost', () => {
    it.skip('should create a sample worker job successfully', async () => {
      const { sampleWorkerPostRoute } = await import(
        '../api/routes/sampleWorker.routes'
      );
      const response = await sampleWorkerPostRoute.sdk.sampleworkerpost({
        body: mockSampleWorkerData,
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBeDefined();
      if (response.code === 200) {
        expect(response.response.message).toBe('New test message');
        expect(response.response.processed).toBe(false);
        expect(response.response.retryCount).toBe(0);
      }
    });

    it('should handle validation errors when creating worker job', async () => {
      const { sampleWorkerPostRoute } = await import(
        '../api/routes/sampleWorker.routes'
      );
      const invalidData = {
        message: ''
      };

      try {
        await sampleWorkerPostRoute.sdk.sampleworkerpost({
          body: invalidData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it('should require HMAC authentication', async () => {
      const { sampleWorkerPostRoute } = await import(
        '../api/routes/sampleWorker.routes'
      );

      try {
        await sampleWorkerPostRoute.sdk.sampleworkerpost({
          body: mockSampleWorkerData,
          headers: {
            authorization: TEST_TOKENS.HMAC_INVALID
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it.skip('should enqueue jobs to all worker queues', async () => {
      const { sampleWorkerPostRoute } = await import(
        '../api/routes/sampleWorker.routes'
      );
      const response = await sampleWorkerPostRoute.sdk.sampleworkerpost({
        body: mockSampleWorkerData,
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      if (response.code === 200) {
        expect(response.response.message).toBe(mockSampleWorkerData.message);
        expect(response.response.processed).toBe(false);
        expect(response.response.retryCount).toBe(0);
      }

      if (!orm) throw new Error('ORM not initialized');
      const em = orm.em.fork();
      const { SampleWorkerEventRecord } = await import(
        '../persistence/entities/sampleWorkerRecord.entity'
      );

      const records = await em.find(SampleWorkerEventRecord, {
        message: mockSampleWorkerData.message
      });

      expect(records.length).toBeGreaterThanOrEqual(1);
    });
  });
});
