import {
  cleanupTestDatabase,
  clearDatabase,
  setupTestData,
  setupTestDatabase,
  TestSetupResult
} from './test-utils';

describe('Twilio Webhook Routes E2E Tests with PostgreSQL Container', () => {
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

  describe('POST /webhook - handleStatusCallback', () => {
    it('should map a delivered status callback onto the seeded record', async () => {
      const { handleStatusCallbackRoute } = await import(
        '../api/routes/webhook.routes'
      );

      const response =
        await handleStatusCallbackRoute.sdk.handleStatusCallback({
          body: {
            urlEncodedForm: {
              MessageSid: 'SM_test_123',
              MessageStatus: 'delivered'
            }
          }
        });

      expect(response.code).toBe(200);

      if (!orm) throw new Error('ORM not initialized');
      const em = orm.em.fork();
      const { SmsRecord } = await import(
        '../persistence/entities/smsRecord.entity'
      );
      const record = await em.findOneOrFail(SmsRecord, {
        providerMessageId: 'SM_test_123'
      });
      expect(record.status).toBe('delivered');
    });

    it('should map a failed status callback with an error message', async () => {
      const { handleStatusCallbackRoute } = await import(
        '../api/routes/webhook.routes'
      );

      const response =
        await handleStatusCallbackRoute.sdk.handleStatusCallback({
          body: {
            urlEncodedForm: {
              MessageSid: 'SM_test_123',
              MessageStatus: 'failed',
              ErrorMessage: 'Unreachable destination handset'
            }
          }
        });

      expect(response.code).toBe(200);

      if (!orm) throw new Error('ORM not initialized');
      const em = orm.em.fork();
      const { SmsRecord } = await import(
        '../persistence/entities/smsRecord.entity'
      );
      const record = await em.findOneOrFail(SmsRecord, {
        providerMessageId: 'SM_test_123'
      });
      expect(record.status).toBe('failed');
      expect(record.error).toBe('Unreachable destination handset');
    });
  });
});
