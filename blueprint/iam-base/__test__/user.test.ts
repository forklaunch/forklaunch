import { OrganizationStatus } from '../domain/enum/organizationStatus.enum';
import { Organization } from '../persistence/entities/organization.entity';
import { User } from '../persistence/entities/user.entity';
import {
  cleanupTestDatabase,
  clearDatabase,
  mockPermissionResponse,
  mockRoleResponse,
  mockUpdateUserData,
  mockUserData,
  mountWithClaimsAuth,
  setupTestData,
  setupTestDatabase,
  TEST_TOKENS,
  TestSetupResult,
  withMiddlewares
} from './test-utils';

// The user setupTestData creates (organization 123e4567-...-426614174001).
const TEST_USER_ID = '123e4567-e89b-12d3-a456-426614174000';

describe('User Routes E2E Tests with PostgreSQL Container', () => {
  let orm: TestSetupResult['orm'];
  let jwt: NonNullable<TestSetupResult['jwt']>;
  // A session for the test user. The contract gates reads on the
  // `platform:read` permission and writes on a `platform:read` role.
  let authorization: `Bearer ${string}`;

  beforeAll(async () => {
    const setup = await setupTestDatabase();
    orm = setup.orm;
    if (!setup.jwt) throw new Error('JWT issuer not started');
    jwt = setup.jwt;
    authorization = await jwt.bearer({
      sub: TEST_USER_ID,
      roles: ['platform:read'],
      permissions: ['platform:read']
    });
    const { userRouter } = await import('../api/routes/user.routes');
    await mountWithClaimsAuth(userRouter);
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

  describe('POST /user - createUser', () => {
    it('should create a user successfully', async () => {
      const { createUserRoute } = await import('../api/routes/user.routes');
      const response = await createUserRoute.sdk.createUser({
        body: mockUserData,
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(201);
      expect(response.response).toBe('User created successfully');
    });

    it('should handle validation errors', async () => {
      const { createUserRoute } = await import('../api/routes/user.routes');
      const invalidData = {
        email: 'invalid-email',
        password: '123',
        firstName: '',
        lastName: '',
        organization: '',
        roles: []
      };

      try {
        await createUserRoute.sdk.createUser({
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
  });

  describe('POST /user/batch - createBatchUsers', () => {
    it('should create multiple users successfully', async () => {
      const { createBatchUsersRoute } = await import(
        '../api/routes/user.routes'
      );
      const batchData = [
        mockUserData,
        {
          ...mockUserData,
          email: 'test2@example.com',
          firstName: 'Jane',
          subscription: 'basic'
        }
      ];

      const response = await createBatchUsersRoute.sdk.createBatchUsers({
        body: batchData,
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(201);
      expect(response.response).toBe('Batch users created successfully');
    });
  });

  describe('GET /user/:id - getUser', () => {
    it('should get a user successfully', async () => {
      const { getUserRoute } = await import('../api/routes/user.routes');
      const response = await withMiddlewares(getUserRoute.sdk.getUser)({
        params: { id: '123e4567-e89b-12d3-a456-426614174000' },
        headers: {
          authorization
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBeDefined();
    });
  });

  describe('GET /user/batch - getBatchUsers', () => {
    it('should get multiple users successfully', async () => {
      const { getBatchUsersRoute } = await import('../api/routes/user.routes');
      const response = await withMiddlewares(
        getBatchUsersRoute.sdk.getBatchUsers
      )({
        query: {
          ids: [
            '123e4567-e89b-12d3-a456-426614174000',
            '456e7890-e89b-12d3-a456-426614174001'
          ]
        },
        headers: {
          authorization
        }
      });

      expect(response.code).toBe(200);
      expect(Array.isArray(response.response)).toBe(true);
    });

    it('should filter users by organization at service level', async () => {
      if (!orm) throw new Error('ORM not initialized');
      const em = orm.em.fork();

      const org1Id = '111e1111-e11b-11d1-a111-111111111111';
      const org2Id = '222e2222-e22b-22d2-a222-222222222222';
      const user1Id = 'aaa11111-e11b-11d1-a111-111111111111';
      const user2Id = 'bbb22222-e22b-22d2-a222-222222222222';

      const org1 = em.create(Organization, {
        id: org1Id,
        name: 'Organization 1',
        domain: 'org1.example.com',
        subscription: 'sub-org1-test',
        status: OrganizationStatus.ACTIVE,
        providerFields: null
      });

      const org2 = em.create(Organization, {
        id: org2Id,
        name: 'Organization 2',
        domain: 'org2.example.com',
        subscription: 'sub-org2-test',
        status: OrganizationStatus.ACTIVE,
        providerFields: null
      });

      const user1 = em.create(User, {
        id: user1Id,
        email: 'user1@org1.com',
        firstName: 'User',
        lastName: 'One',
        phoneNumber: '+1234567890',
        subscription: 'sub-user1-test',
        organization: org1,
        providerFields: null
      });

      const user2 = em.create(User, {
        id: user2Id,
        email: 'user2@org2.com',
        firstName: 'User',
        lastName: 'Two',
        phoneNumber: '+0987654321',
        subscription: 'sub-user2-test',
        organization: org2,
        providerFields: null
      });

      await em.persist([org1, org2, user1, user2]).flush();

      const { ci, tokens } = await import('../bootstrapper');
      const UserService = ci.resolve(tokens.UserService);

      const result = await UserService.getBatchUsers(
        {
          ids: [user1Id, user2Id],
          organization: { id: org1Id }
        } as Parameters<typeof UserService.getBatchUsers>[0],
        em
      );

      expect(result).toHaveLength(1);
      expect(result[0].id).toBe(user1Id);
      expect(result[0].email).toBe('user1@org1.com');
    });
  });

  describe('PUT /user - updateUser', () => {
    it('should update a user successfully', async () => {
      const { updateUserRoute } = await import('../api/routes/user.routes');
      const response = await withMiddlewares(updateUserRoute.sdk.updateUser)({
        body: mockUpdateUserData,
        headers: {
          authorization
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBe('User updated successfully');
    });
  });

  describe('PUT /user/batch - updateBatchUsers', () => {
    it('should update multiple users successfully', async () => {
      const { updateBatchUsersRoute } = await import(
        '../api/routes/user.routes'
      );
      const batchUpdateData = [mockUpdateUserData];

      const response = await withMiddlewares(
        updateBatchUsersRoute.sdk.updateBatchUsers
      )({
        body: batchUpdateData,
        headers: {
          authorization
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBe('Batch users updated successfully');
    });
  });

  describe('DELETE /user/:id - deleteUser', () => {
    it('should delete a user successfully', async () => {
      const { deleteUserRoute } = await import('../api/routes/user.routes');
      const response = await withMiddlewares(deleteUserRoute.sdk.deleteUser)({
        params: { id: '123e4567-e89b-12d3-a456-426614174000' },
        headers: {
          authorization
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBe('User deleted successfully');
    });
  });

  describe('DELETE /user/batch - deleteBatchUsers', () => {
    it('should delete multiple users successfully', async () => {
      const { deleteBatchUsersRoute } = await import(
        '../api/routes/user.routes'
      );
      const response = await withMiddlewares(
        deleteBatchUsersRoute.sdk.deleteBatchUsers
      )({
        query: {
          ids: ['123e4567-e89b-12d3-a456-426614174000']
        },
        headers: {
          authorization
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBe('Batch users deleted successfully');
    });
  });

  describe('GET /user/:id/surface-roles - surfaceRoles', () => {
    it('should get user roles successfully', async () => {
      const { surfaceRolesRoute } = await import('../api/routes/user.routes');
      const response = await surfaceRolesRoute.sdk.surfaceUserRoles({
        params: { id: '123e4567-e89b-12d3-a456-426614174000' },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      // Entities carry fields the schema does not list (retention
      // bookkeeping), so match the schema's fields.
      expect(response.response).toMatchObject(mockRoleResponse);
    });
  });

  describe('GET /user/:id/surface-permissions - surfacePermissions', () => {
    it('should get user permissions successfully', async () => {
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
      // Entities carry fields the schema does not list (retention
      // bookkeeping), so match the schema's fields.
      expect(response.response).toMatchObject(mockPermissionResponse);
    });
  });

  describe('JWT authentication', () => {
    it('rejects a request without a token', async () => {
      const { getUserRoute } = await import('../api/routes/user.routes');
      const response = await withMiddlewares(getUserRoute.sdk.getUser)({
        params: { id: TEST_USER_ID },
        headers: {}
      } as Parameters<typeof getUserRoute.sdk.getUser>[0]);

      expect(response.code).toBe(401);
    });

    it('rejects an expired token', async () => {
      const { getUserRoute } = await import('../api/routes/user.routes');
      const response = await withMiddlewares(getUserRoute.sdk.getUser)({
        params: { id: TEST_USER_ID },
        headers: {
          authorization: await jwt.bearer({
            sub: TEST_USER_ID,
            permissions: ['platform:read'],
            expiresIn: -60
          })
        }
      });

      expect([401, 403]).toContain(response.code);
    });

    it('rejects a session without the required permission', async () => {
      const { getUserRoute } = await import('../api/routes/user.routes');
      const response = await withMiddlewares(getUserRoute.sdk.getUser)({
        params: { id: TEST_USER_ID },
        headers: {
          authorization: await jwt.bearer({
            sub: TEST_USER_ID,
            permissions: []
          })
        }
      });

      expect(response.code).toBe(403);
    });
  });
});
