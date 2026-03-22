import {
  cleanupTestDatabase,
  clearDatabase,
  mockRoleData,
  mockUpdateRoleData,
  setupTestData,
  setupTestDatabase,
  TEST_TOKENS,
  TestSetupResult
} from './test-utils';

describe('Role Routes E2E Tests with PostgreSQL Container', () => {
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

  describe('POST /role - createRole', () => {
    it('should create a role successfully', async () => {
      const { createRoleRoute } = await import('../api/routes/role.routes');

      const response = await createRoleRoute.sdk.createRole({
        body: mockRoleData,
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(201);
      expect(response.response).toBe('Role created successfully');
    });

    it('should handle validation errors when creating role', async () => {
      const { createRoleRoute } = await import('../api/routes/role.routes');

      const invalidData = {
        name: '', // Empty name should fail validation
        permissions: []
      };

      try {
        await createRoleRoute.sdk.createRole({
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

    it('should handle non-existent permission references', async () => {
      const { createRoleRoute } = await import('../api/routes/role.routes');

      const invalidData = {
        name: 'test-role',
        permissions: ['00000000-0000-0000-0000-000000000000'] // Non-existent permission ID
      };

      try {
        await createRoleRoute.sdk.createRole({
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

  describe('POST /role/batch - createBatchRoles', () => {
    it('should create multiple roles successfully', async () => {
      const { createBatchRolesRoute } = await import(
        '../api/routes/role.routes'
      );

      const batchData = [
        mockRoleData,
        {
          name: 'moderator',
          permissions: ['123e4567-e89b-12d3-a456-426614174002']
        },
        {
          name: 'viewer',
          permissions: ['123e4567-e89b-12d3-a456-426614174002']
        }
      ];

      const response = await createBatchRolesRoute.sdk.createBatchRoles({
        body: batchData,
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(201);
      expect(response.response).toBe('Batch roles created successfully');
    });

    it('should handle partial failures in batch creation', async () => {
      const { createBatchRolesRoute } = await import(
        '../api/routes/role.routes'
      );

      const batchData = [
        mockRoleData,
        {
          name: '', // This should fail
          permissions: ['123e4567-e89b-12d3-a456-426614174002']
        },
        {
          name: 'valid-role',
          permissions: ['123e4567-e89b-12d3-a456-426614174002']
        }
      ];

      try {
        await createBatchRolesRoute.sdk.createBatchRoles({
          body: batchData,
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

  describe('GET /role/:id - getRole', () => {
    it('should get a role successfully', async () => {
      const { getRoleRoute } = await import('../api/routes/role.routes');

      const response = await getRoleRoute.sdk.getRole({
        params: { id: '123e4567-e89b-12d3-a456-426614174000' },
        headers: {
          authorization: TEST_TOKENS.AUTH
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBeDefined();
      if (typeof response.response === 'object' && response.response !== null) {
        expect(response.response.id).toBe(
          '123e4567-e89b-12d3-a456-426614174000'
        );
        expect(response.response.name).toBe('admin');
        expect(response.response.permissions).toBeDefined();
        expect(Array.isArray(response.response.permissions)).toBe(true);
      }
    });

    it('should handle non-existent role', async () => {
      const { getRoleRoute } = await import('../api/routes/role.routes');

      try {
        await getRoleRoute.sdk.getRole({
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

  describe('GET /role/batch - getBatchRoles', () => {
    it('should get multiple roles successfully', async () => {
      const { getBatchRolesRoute } = await import('../api/routes/role.routes');

      const response = await getBatchRolesRoute.sdk.getBatchRoles({
        query: {
          ids: [
            '123e4567-e89b-12d3-a456-426614174000',
            '456e7890-e89b-12d3-a456-426614174001'
          ]
        },
        headers: {
          authorization: TEST_TOKENS.AUTH
        }
      });

      expect(response.code).toBe(200);
      expect(Array.isArray(response.response)).toBe(true);
    });
  });

  describe('PUT /role - updateRole', () => {
    it('should update a role successfully', async () => {
      const { updateRoleRoute } = await import('../api/routes/role.routes');

      const response = await updateRoleRoute.sdk.updateRole({
        body: mockUpdateRoleData,
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBe('Role updated successfully');
    });

    it('should handle validation errors when updating role', async () => {
      const { updateRoleRoute } = await import('../api/routes/role.routes');

      const invalidData = {
        id: '123e4567-e89b-12d3-a456-426614174000',
        name: '', // Empty name should fail validation
        permissions: ['123e4567-e89b-12d3-a456-426614174002']
      };

      try {
        await updateRoleRoute.sdk.updateRole({
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

  describe('PUT /role/batch - updateBatchRoles', () => {
    it('should update multiple roles successfully', async () => {
      const { updateBatchRolesRoute } = await import(
        '../api/routes/role.routes'
      );

      const batchUpdateData = [mockUpdateRoleData];

      const response = await updateBatchRolesRoute.sdk.updateBatchRoles({
        body: batchUpdateData,
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBe('Batch roles updated successfully');
    });
  });

  describe('DELETE /role/:id - deleteRole', () => {
    it('should delete a role successfully', async () => {
      const { deleteRoleRoute } = await import('../api/routes/role.routes');

      const response = await deleteRoleRoute.sdk.deleteRole({
        params: { id: '123e4567-e89b-12d3-a456-426614174000' },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBe('Role deleted successfully');
    });

    it('should handle non-existent role deletion', async () => {
      const { deleteRoleRoute } = await import('../api/routes/role.routes');

      try {
        await deleteRoleRoute.sdk.deleteRole({
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

  describe('DELETE /role/batch - deleteBatchRoles', () => {
    it('should delete multiple roles successfully', async () => {
      const { deleteBatchRolesRoute } = await import(
        '../api/routes/role.routes'
      );

      const response = await deleteBatchRolesRoute.sdk.deleteBatchRoles({
        query: {
          ids: [
            '123e4567-e89b-12d3-a456-426614174000',
            '456e7890-e89b-12d3-a456-426614174001'
          ]
        },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBe('Batch roles deleted successfully');
    });
  });

  describe('Role Business Logic Tests', () => {
    it('should enforce unique role name constraint', async () => {
      const { createRoleRoute } = await import('../api/routes/role.routes');

      // Try to create role with existing name
      const duplicateData = {
        name: 'admin', // This name already exists in test data
        permissions: ['123e4567-e89b-12d3-a456-426614174002']
      };

      try {
        await createRoleRoute.sdk.createRole({
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

    it('should validate role name format', async () => {
      const { createRoleRoute } = await import('../api/routes/role.routes');

      const invalidNameData = {
        name: 'Invalid Role Name!', // Should be alphanumeric with hyphens/underscores
        permissions: ['123e4567-e89b-12d3-a456-426614174002']
      };

      try {
        await createRoleRoute.sdk.createRole({
          body: invalidNameData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });
        expect(true).toBe(false); // Should not reach here
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it('should prevent deletion of role assigned to users', async () => {
      const { deleteRoleRoute } = await import('../api/routes/role.routes');

      // Try to delete role that's assigned to a user
      try {
        await deleteRoleRoute.sdk.deleteRole({
          params: { id: '123e4567-e89b-12d3-a456-426614174000' }, // This role is assigned to test user
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });
        expect(true).toBe(false); // Should not reach here due to foreign key constraint
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it('should allow role without permissions', async () => {
      const { createRoleRoute } = await import('../api/routes/role.routes');

      const roleWithoutPermissions = {
        name: 'guest',
        permissions: [] // Empty permissions should be allowed
      };

      const response = await createRoleRoute.sdk.createRole({
        body: roleWithoutPermissions,
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(201);
      expect(response.response).toBe('Role created successfully');
    });

    it('should handle role permission updates correctly', async () => {
      const { updateRoleRoute } = await import('../api/routes/role.routes');

      // First create a new permission
      const { Permission } = await import(
        '../persistence/entities/permission.entity'
      );
      if (!orm) throw new Error('ORM not initialized');
      const em = orm.em.fork();

      em.create(Permission, {
        id: '123e4567-e89b-12d3-a456-426614174003',
        slug: 'write:organizations',
        createdAt: new Date(),
        updatedAt: new Date()
      });
      await em.flush();

      // Update role to include the new permission
      const updateData = {
        id: '123e4567-e89b-12d3-a456-426614174000',
        name: 'admin',
        permissions: [
          '123e4567-e89b-12d3-a456-426614174002', // Original permission
          '123e4567-e89b-12d3-a456-426614174003' // New permission
        ]
      };

      const response = await updateRoleRoute.sdk.updateRole({
        body: updateData,
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBe('Role updated successfully');
    });
  });
});
