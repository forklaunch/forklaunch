import {
  cleanupTestDatabase,
  clearDatabase,
  mockPermissionResponse,
  mockRoleResponse,
  setupTestData,
  setupTestDatabase,
  TEST_TOKENS,
  TestSetupResult
} from './test-utils';

describe('User Surfacing Routes E2E Tests with PostgreSQL Container', () => {
  let orm: TestSetupResult['orm'];

  beforeAll(async () => {
    const setup = await setupTestDatabase();
    orm = setup.orm;
  }, 60000);

  beforeEach(async () => {
    await clearDatabase({ orm });
    if (!orm) throw new Error('ORM not initialized');
    const em = orm.em.fork();
    await setupTestData(em);
  });

  afterAll(async () => {
    await cleanupTestDatabase();
  }, 30000);

  describe('GET /user/:id/surface-roles - surfaceRoles', () => {
    it('should get user roles from active org successfully', async () => {
      const { surfaceRolesRoute } = await import('../api/routes/user.routes');
      const response = await surfaceRolesRoute.sdk.surfaceUserRoles({
        params: { id: '123e4567-e89b-12d3-a456-426614174000' },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toEqual(mockRoleResponse);
    });

    it('should return empty array for user with no active session', async () => {
      const { surfaceRolesRoute } = await import('../api/routes/user.routes');
      const response = await surfaceRolesRoute.sdk.surfaceUserRoles({
        params: { id: '00000000-0000-0000-0000-000000000000' },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toEqual([]);
    });
  });

  describe('GET /user/:id/surface-permissions - surfacePermissions', () => {
    it('should get user permissions from active org successfully', async () => {
      const { surfacePermissionsRoute } = await import(
        '../api/routes/user.routes'
      );
      const response = await surfacePermissionsRoute.sdk.surfaceUserPermissions(
        {
          params: { id: '123e4567-e89b-12d3-a456-426614174000' },
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        }
      );

      expect(response.code).toBe(200);
      expect(response.response).toEqual(
        expect.arrayContaining(mockPermissionResponse)
      );
    });

    it('should return empty array for user with no active session', async () => {
      const { surfacePermissionsRoute } = await import(
        '../api/routes/user.routes'
      );
      const response = await surfacePermissionsRoute.sdk.surfaceUserPermissions(
        {
          params: { id: '00000000-0000-0000-0000-000000000000' },
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        }
      );

      expect(response.code).toBe(200);
      expect(response.response).toEqual([]);
    });
  });
});
