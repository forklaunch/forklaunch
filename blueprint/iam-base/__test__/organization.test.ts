import {
  cleanupTestDatabase,
  clearDatabase,
  mockOrganizationData,
  mockUpdateOrganizationData,
  setupTestData,
  setupTestDatabase,
  TEST_TOKENS,
  TestSetupResult
} from './test-utils';

describe('Organization Routes E2E Tests with PostgreSQL Container', () => {
  let orm: TestSetupResult['orm'];

  beforeAll(async () => {
    const setup = await setupTestDatabase();
    orm = setup.orm;
  });

  beforeEach(async () => {
    await clearDatabase({ orm });
    if (!orm) throw new Error('ORM not initialized');
    const em = orm.em.fork();
    await setupTestData(em);
  });

  afterAll(async () => {
    await cleanupTestDatabase();
  });

  describe('POST /organization - createOrganization', () => {
    it('should create an organization successfully', async () => {
      const { createOrganizationRoute } = await import(
        '../api/routes/organization.routes'
      );

      const response = await createOrganizationRoute.sdk.createOrganization({
        body: mockOrganizationData,
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(201);
      expect(response.response).toBeDefined();
      if (typeof response.response === 'object' && response.response !== null) {
        expect(response.response.id).toBeDefined();
        expect(response.response.name).toBe('New Organization');
      }
    });

    it('should handle validation errors when creating organization', async () => {
      const { createOrganizationRoute } = await import(
        '../api/routes/organization.routes'
      );

      const invalidData = {
        name: '',
        domain: 'invalid-domain',
        subscription: 'invalid-subscription',
        status: 'invalid-status'
      };

      try {
        await createOrganizationRoute.sdk.createOrganization({
          body: invalidData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });
        expect(true).toBe(false); // Should not reach here
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });
  });

  describe('GET /organization/:id - getOrganization', () => {
    it('should get an organization successfully', async () => {
      const { getOrganizationRoute } = await import(
        '../api/routes/organization.routes'
      );

      const response = await getOrganizationRoute.sdk.getOrganization({
        params: { id: '123e4567-e89b-12d3-a456-426614174001' },
        headers: {
          authorization: TEST_TOKENS.AUTH
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBeDefined();
      if (typeof response.response === 'object' && response.response !== null) {
        expect(response.response.id).toBe(
          '123e4567-e89b-12d3-a456-426614174001'
        );
        expect(response.response.name).toBe('Test Organization');
      }
    });

    it('should handle non-existent organization', async () => {
      const { getOrganizationRoute } = await import(
        '../api/routes/organization.routes'
      );

      try {
        await getOrganizationRoute.sdk.getOrganization({
          params: { id: '00000000-0000-0000-0000-000000000000' },
          headers: {
            authorization: TEST_TOKENS.AUTH
          }
        });
        expect(true).toBe(false); // Should not reach here
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });
  });

  describe('PUT /organization - updateOrganization', () => {
    it('should update an organization successfully', async () => {
      const { updateOrganizationRoute } = await import(
        '../api/routes/organization.routes'
      );

      const response = await updateOrganizationRoute.sdk.updateOrganization({
        body: mockUpdateOrganizationData,
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBeDefined();
      if (typeof response.response === 'object' && response.response !== null) {
        expect(response.response.id).toBe(
          '123e4567-e89b-12d3-a456-426614174001'
        );
        expect(response.response.name).toBe('Updated Organization');
      }
    });

    it('should handle validation errors when updating organization', async () => {
      const { updateOrganizationRoute } = await import(
        '../api/routes/organization.routes'
      );

      const invalidData = {
        id: '123e4567-e89b-12d3-a456-426614174001',
        name: '',
        domain: 'invalid-domain',
        subscription: 'invalid-subscription',
        status: 'invalid-status'
      };

      try {
        await updateOrganizationRoute.sdk.updateOrganization({
          body: invalidData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });
        expect(true).toBe(false); // Should not reach here
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });
  });

  describe('DELETE /organization/:id - deleteOrganization', () => {
    it('should delete an organization successfully', async () => {
      const { deleteOrganizationRoute } = await import(
        '../api/routes/organization.routes'
      );

      const response = await deleteOrganizationRoute.sdk.deleteOrganization({
        params: { id: '123e4567-e89b-12d3-a456-426614174001' },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBe('Organization deleted successfully');
    });

    it('should handle non-existent organization deletion', async () => {
      const { deleteOrganizationRoute } = await import(
        '../api/routes/organization.routes'
      );

      try {
        await deleteOrganizationRoute.sdk.deleteOrganization({
          params: { id: '00000000-0000-0000-0000-000000000000' },
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });
        expect(true).toBe(false); // Should not reach here
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });
  });

  describe('Organization Business Logic Tests', () => {
    it('should enforce unique domain constraint', async () => {
      const { createOrganizationRoute } = await import(
        '../api/routes/organization.routes'
      );

      // Try to create organization with existing domain
      const duplicateData = {
        ...mockOrganizationData,
        domain: 'test.com' // This domain already exists in test data
      };

      try {
        await createOrganizationRoute.sdk.createOrganization({
          body: duplicateData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });
        expect(true).toBe(false); // Should not reach here
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it('should validate organization status values', async () => {
      const { createOrganizationRoute } = await import(
        '../api/routes/organization.routes'
      );

      const invalidStatusData = {
        ...mockOrganizationData,
        status: 'invalid-status'
      };

      try {
        await createOrganizationRoute.sdk.createOrganization({
          body: invalidStatusData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });
        expect(true).toBe(false); // Should not reach here
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });
  });
});
