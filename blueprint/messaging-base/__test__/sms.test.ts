import {
  cleanupTestDatabase,
  clearDatabase,
  mockSendSmsData,
  setupTestData,
  setupTestDatabase,
  TEST_TOKENS,
  TestSetupResult
} from './test-utils';

describe('Sms Routes E2E Tests with PostgreSQL Container', () => {
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

  describe('POST /sms - sendSms', () => {
    it('should send an sms successfully and mark it sent', async () => {
      const { sendSmsRoute } = await import('../api/routes/sms.routes');

      const response = await sendSmsRoute.sdk.sendSms({
        body: mockSendSmsData,
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toMatchObject({
        to: mockSendSmsData.to,
        body: mockSendSmsData.body,
        metadata: mockSendSmsData.metadata,
        status: 'sent',
        id: expect.any(String),
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date)
      });
    });

    it('should handle validation errors when sending an sms', async () => {
      const { sendSmsRoute } = await import('../api/routes/sms.routes');

      const invalidData = {
        to: '+15555550199'
      };

      try {
        await sendSmsRoute.sdk.sendSms({
          body: invalidData as typeof mockSendSmsData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });
  });

  describe('GET /sms/:id - getSmsRecord', () => {
    it('should retrieve a persisted sms record', async () => {
      const { getSmsRecordRoute } = await import('../api/routes/sms.routes');

      const response = await getSmsRecordRoute.sdk.getSmsRecord({
        params: { id: '123e4567-e89b-12d3-a456-426614174001' },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toMatchObject({
        id: '123e4567-e89b-12d3-a456-426614174001',
        to: '+15555550100',
        body: 'A seeded sms record',
        status: 'sent',
        providerMessageId: 'SM_test_123'
      });
    });

    it('should fail for a missing sms record', async () => {
      const { getSmsRecordRoute } = await import('../api/routes/sms.routes');

      try {
        await getSmsRecordRoute.sdk.getSmsRecord({
          params: { id: '123e4567-e89b-12d3-a456-426614174999' },
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });
  });
});
