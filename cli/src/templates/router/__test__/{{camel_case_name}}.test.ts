import {
  cleanupTestDatabase,
  clearDatabase,
  mock{{pascal_case_name}}Data,
  {{#is_database_enabled}}setupTestData,
  {{/is_database_enabled}}setupTestDatabase,
  TestSetupResult
} from './test-utils';

describe('{{pascal_case_name}} Routes E2E Tests', () => {
  {{#is_database_enabled}}let orm: TestSetupResult['orm'];{{/is_database_enabled}}
  {{#is_cache_enabled}}let redis: TestSetupResult['redis'];{{/is_cache_enabled}}
  {{#is_kafka_enabled}}let kafkaContainer: TestSetupResult['kafkaContainer'];{{/is_kafka_enabled}}
  {{#is_s3_enabled}}let s3Container: TestSetupResult['s3Container'];{{/is_s3_enabled}}

  beforeAll(async () => {
    const setup = await setupTestDatabase();
    {{#is_database_enabled}}orm = setup.orm;{{/is_database_enabled}}
    {{#is_cache_enabled}}redis = setup.redis;{{/is_cache_enabled}}
    {{#is_kafka_enabled}}kafkaContainer = setup.kafkaContainer;{{/is_kafka_enabled}}
    {{#is_s3_enabled}}s3Container = setup.s3Container;{{/is_s3_enabled}}
  }, 60000);

  beforeEach(async () => {
    await clearDatabase({{#is_database_enabled}}{{#is_cache_enabled}}{ orm, redis }{{/is_cache_enabled}}{{^is_cache_enabled}}{ orm }{{/is_cache_enabled}}{{/is_database_enabled}}{{^is_database_enabled}}{{#is_cache_enabled}}{ redis }{{/is_cache_enabled}}{{/is_database_enabled}});
    {{#is_database_enabled}}if (!orm) throw new Error('ORM not initialized');
    const em = orm.em.fork();
    await setupTestData(em);{{/is_database_enabled}}
  });

  afterAll(async () => {
    await cleanupTestDatabase();
  }, 30000);

  describe('GET /{{camel_case_name}} - {{camel_case_name}}Get', () => {
    it('should handle {{camel_case_name}} get request successfully', async () => {
      const { {{camel_case_name}}GetRoute } = await import(
        '../api/routes/{{camel_case_name}}.routes'
      );

      const response = await {{camel_case_name}}GetRoute.sdk.{{camel_case_name}}Get({{#is_iam_configured}}{ 
        headers: {
          authorization: `Bearer test-token`
        }
      }{{/is_iam_configured}});

      expect(response.code).toBe(200);
      expect(response.response).toBeDefined();
    });
  });

  describe('POST /{{camel_case_name}} - {{camel_case_name}}Post', () => {
    it('should handle {{camel_case_name}} request successfully', async () => {
      const { {{camel_case_name}}PostRoute } = await import(
        '../api/routes/{{camel_case_name}}.routes'
      );

      const response = await {{camel_case_name}}PostRoute.sdk.{{camel_case_name}}Post({
        body: mock{{pascal_case_name}}Data{{#is_iam_configured}},
        headers: {
          authorization: `Bearer test-token`
        }{{/is_iam_configured}}
      });

      expect(response.code).toBe(200);
      expect(response.response).toBeDefined();
      if (response.code === 200) {
        expect(response.response.message).toBeDefined();
      }
    });

    it('should handle validation errors', async () => {
      const { {{camel_case_name}}PostRoute } = await import(
        '../api/routes/{{camel_case_name}}.routes'
      );

      const invalidData = {
        message: ''
      };

      try {
        await {{camel_case_name}}PostRoute.sdk.{{camel_case_name}}Post({
          body: invalidData{{#is_iam_configured}},
          headers: {
            authorization: `Bearer test-token`
          }{{/is_iam_configured}}
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });
    {{#is_database_enabled}}

    it('should persist data to database', async () => {
      const { {{camel_case_name}}PostRoute } = await import(
        '../api/routes/{{camel_case_name}}.routes'
      );

      await {{camel_case_name}}PostRoute.sdk.{{camel_case_name}}Post({
        body: mock{{pascal_case_name}}Data{{#is_iam_configured}},
        headers: {
          authorization: `Bearer test-token`
        }{{/is_iam_configured}}
      });

      if (!orm) throw new Error('ORM not initialized');
      const em = orm.em.fork();
      const { {{pascal_case_name}}{{#is_worker}}Event{{/is_worker}}Record } = await import(
        '../persistence/entities/{{camel_case_name}}{{#is_worker}}Event{{/is_worker}}Record.entity'
      );

      const records = await em.find({{pascal_case_name}}{{#is_worker}}Event{{/is_worker}}Record, {
        message: mock{{pascal_case_name}}Data.message
      });

      expect(records.length).toBeGreaterThan(0);
    });
    {{/is_database_enabled}}
  });
});
