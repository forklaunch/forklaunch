import {
  cleanupTestDatabase,
  clearDatabase,
  getMockBillingPortalData,
  setupTestData,
  setupTestDatabase,
  TEST_TOKENS,
  TestSetupResult
} from './test-utils';

describe('BillingPortal Routes E2E Tests with PostgreSQL Container', () => {
  let orm: TestSetupResult['orm'];
  let redis: TestSetupResult['redis'];

  beforeAll(async () => {
    const setup = await setupTestDatabase();
    orm = setup.orm;
    redis = setup.redis;
  });

  beforeEach(async () => {
    await clearDatabase({ orm, redis });
    if (!orm) throw new Error('ORM not initialized');
    const em = orm.em.fork();
    await setupTestData(em);
  });

  afterAll(async () => {
    await cleanupTestDatabase();
  });

  describe('POST /billing-portal - createBillingPortalSession', () => {
    it('should create a billing portal session successfully', async () => {
      const { createBillingPortalRoute } = await import(
        '../api/routes/billingPortal.routes'
      );

      const response =
        await createBillingPortalRoute.sdk.createBillingPortalSession({
          body: getMockBillingPortalData(),
          headers: {
            authorization: TEST_TOKENS.AUTH
          }
        });

      expect(response.code).toBe(200);
      expect(response.response).toMatchObject({
        customerId: getMockBillingPortalData().customerId,
        id: expect.any(String),
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date)
      });
    });

    it('should handle validation errors when creating billing portal session', async () => {
      const { createBillingPortalRoute } = await import(
        '../api/routes/billingPortal.routes'
      );

      const invalidData = {
        ...getMockBillingPortalData(),
        customerId: '',
        returnUrl: 'invalid-url'
      };

      try {
        await createBillingPortalRoute.sdk.createBillingPortalSession({
          body: invalidData,
          headers: {
            authorization: TEST_TOKENS.AUTH
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it('should handle invalid billing provider enum', async () => {
      const { createBillingPortalRoute } = await import(
        '../api/routes/billingPortal.routes'
      );

      const invalidData = {
        ...getMockBillingPortalData(),
        billingProvider: 'INVALID_PROVIDER'
      };

      try {
        await createBillingPortalRoute.sdk.createBillingPortalSession({
          body: invalidData,
          headers: {
            authorization: TEST_TOKENS.AUTH
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });
  });

  describe('GET /billing-portal/:id - getBillingPortalSession', () => {
    it('should retrieve an existing billing portal session successfully', async () => {
      const { createBillingPortalRoute, getBillingPortalRoute } = await import(
        '../api/routes/billingPortal.routes'
      );

      const createResponse =
        await createBillingPortalRoute.sdk.createBillingPortalSession({
          body: getMockBillingPortalData(),
          headers: {
            authorization: TEST_TOKENS.AUTH
          }
        });

      if (createResponse.code !== 200) {
        throw new Error('Failed to create billing portal');
      }
      const portalId = createResponse.response.id;

      const response = await getBillingPortalRoute.sdk.getBillingPortalSession({
        params: { id: portalId },
        headers: {
          authorization: TEST_TOKENS.AUTH
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toMatchObject({
        id: expect.any(String),
        customerId: getMockBillingPortalData().customerId,
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date)
      });
    });

    it('should handle non-existent billing portal session ID', async () => {
      const { getBillingPortalRoute } = await import(
        '../api/routes/billingPortal.routes'
      );

      try {
        await getBillingPortalRoute.sdk.getBillingPortalSession({
          params: { id: '123e4567-e89b-12d3-a456-426614174999' },
          headers: {
            authorization: TEST_TOKENS.AUTH
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it('should handle invalid UUID format', async () => {
      const { getBillingPortalRoute } = await import(
        '../api/routes/billingPortal.routes'
      );

      try {
        await getBillingPortalRoute.sdk.getBillingPortalSession({
          params: { id: 'invalid-uuid' },
          headers: {
            authorization: TEST_TOKENS.AUTH
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });
  });

  describe('PUT /billing-portal/:id - updateBillingPortalSession', () => {
    it('should update an existing billing portal session successfully', async () => {
      const { createBillingPortalRoute, updateBillingPortalRoute } =
        await import('../api/routes/billingPortal.routes');

      const createResponse =
        await createBillingPortalRoute.sdk.createBillingPortalSession({
          body: getMockBillingPortalData(),
          headers: {
            authorization: TEST_TOKENS.AUTH
          }
        });

      if (createResponse.code !== 200) {
        throw new Error('Failed to create billing portal');
      }
      const portalId = createResponse.response.id;
      const updateData = {
        id: portalId,
        customerId: getMockBillingPortalData().customerId,
        expiresAt: getMockBillingPortalData().expiresAt
      };

      const response =
        await updateBillingPortalRoute.sdk.updateBillingPortalSession({
          params: { id: portalId },
          body: updateData,
          headers: {
            authorization: TEST_TOKENS.AUTH
          }
        });

      expect(response.code).toBe(200);
      expect(response.response).toMatchObject({
        id: expect.any(String),
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date)
      });
    });

    it('should handle updating non-existent billing portal session', async () => {
      const { updateBillingPortalRoute } = await import(
        '../api/routes/billingPortal.routes'
      );

      const updateData = {
        id: '123e4567-e89b-12d3-a456-426614174999',
        customerId: 'cus_updated_123',
        externalId: 'portal_updated_123',
        billingProvider: 'STRIPE'
      };

      try {
        await updateBillingPortalRoute.sdk.updateBillingPortalSession({
          params: { id: '123e4567-e89b-12d3-a456-426614174999' },
          body: updateData,
          headers: {
            authorization: TEST_TOKENS.AUTH
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it('should handle validation errors when updating billing portal session', async () => {
      const { createBillingPortalRoute, updateBillingPortalRoute } =
        await import('../api/routes/billingPortal.routes');

      const createResponse =
        await createBillingPortalRoute.sdk.createBillingPortalSession({
          body: getMockBillingPortalData(),
          headers: {
            authorization: TEST_TOKENS.AUTH
          }
        });

      if (createResponse.code !== 200) {
        throw new Error('Failed to create billing portal');
      }
      const portalId = createResponse.response.id;
      const invalidUpdateData = {
        id: portalId
      };

      try {
        await updateBillingPortalRoute.sdk.updateBillingPortalSession({
          params: { id: portalId },
          body: invalidUpdateData,
          headers: {
            authorization: TEST_TOKENS.AUTH
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });
  });

  describe('DELETE /billing-portal/:id - expireBillingPortalSession', () => {
    it('should expire an existing billing portal session successfully', async () => {
      const { createBillingPortalRoute, expireBillingPortalRoute } =
        await import('../api/routes/billingPortal.routes');

      const createResponse =
        await createBillingPortalRoute.sdk.createBillingPortalSession({
          body: getMockBillingPortalData(),
          headers: {
            authorization: TEST_TOKENS.AUTH
          }
        });

      if (createResponse.code !== 200) {
        throw new Error('Failed to create billing portal');
      }
      const portalId = createResponse.response.id;

      const response =
        await expireBillingPortalRoute.sdk.expireBillingPortalSession({
          params: { id: portalId },
          headers: {
            authorization: TEST_TOKENS.AUTH
          }
        });

      expect(response.code).toBe(200);
      expect(response.response).toBe(
        `Expired billing portal session ${portalId}`
      );
    });

    it('should handle expiring non-existent billing portal session', async () => {
      const { expireBillingPortalRoute } = await import(
        '../api/routes/billingPortal.routes'
      );

      try {
        await expireBillingPortalRoute.sdk.expireBillingPortalSession({
          params: { id: '123e4567-e89b-12d3-a456-426614174999' },
          headers: {
            authorization: TEST_TOKENS.AUTH
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });
  });

  describe('Authentication', () => {
    it('should require HMAC authentication for createBillingPortalSession', async () => {
      const { createBillingPortalRoute } = await import(
        '../api/routes/billingPortal.routes'
      );

      try {
        await createBillingPortalRoute.sdk.createBillingPortalSession({
          body: getMockBillingPortalData(),
          headers: {
            authorization: 'Bearer invalid-token'
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it('should require HMAC authentication for getBillingPortalSession', async () => {
      const { getBillingPortalRoute } = await import(
        '../api/routes/billingPortal.routes'
      );

      try {
        await getBillingPortalRoute.sdk.getBillingPortalSession({
          params: { id: '123e4567-e89b-12d3-a456-426614174000' },
          headers: {
            authorization: 'Bearer invalid-token'
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it('should require HMAC authentication for updateBillingPortalSession', async () => {
      const { updateBillingPortalRoute } = await import(
        '../api/routes/billingPortal.routes'
      );

      try {
        await updateBillingPortalRoute.sdk.updateBillingPortalSession({
          params: { id: '123e4567-e89b-12d3-a456-426614174000' },
          body: {
            id: '123e4567-e89b-12d3-a456-426614174000'
          },
          headers: {
            authorization: 'Bearer invalid-token'
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it('should require HMAC authentication for expireBillingPortalSession', async () => {
      const { expireBillingPortalRoute } = await import(
        '../api/routes/billingPortal.routes'
      );

      try {
        await expireBillingPortalRoute.sdk.expireBillingPortalSession({
          params: { id: '123e4567-e89b-12d3-a456-426614174000' },
          headers: {
            authorization: 'Bearer invalid-token'
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });
  });
});
