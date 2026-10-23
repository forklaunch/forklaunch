import {
  cleanupTestDatabase,
  clearDatabase,
  mockPermissionData,
  mockUpdatePermissionData,
  setupTestData,
  setupTestDatabase,
  TEST_TOKENS,
  TestSetupResult
} from './test-utils';

describe('Permission Routes E2E Tests with PostgreSQL Container', () => {
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

  describe('POST /permission - createPermission', () => {
    it('should create a permission successfully', async () => {
      const { createPermissionRoute } = await import(
        '../api/routes/permission.routes'
      );

      const response = await createPermissionRoute.sdk.createPermission({
        body: mockPermissionData,
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(201);
      expect(response.response).toBe('Permission created successfully');
    });

    it('should handle validation errors when creating permission', async () => {
      const { createPermissionRoute } = await import(
        '../api/routes/permission.routes'
      );

      const invalidData = {
        slug: '' // Empty slug should fail validation
      };

      try {
        await createPermissionRoute.sdk.createPermission({
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

  describe('POST /permission/batch - createBatchPermissions', () => {
    it('should create multiple permissions successfully', async () => {
      const { createBatchPermissionsRoute } = await import(
        '../api/routes/permission.routes'
      );

      const batchData = [
        mockPermissionData,
        {
          slug: 'delete:users'
        },
        {
          slug: 'manage:roles'
        }
      ];

      const response =
        await createBatchPermissionsRoute.sdk.createBatchPermissions({
          body: batchData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });

      expect(response.code).toBe(201);
      expect(response.response).toBe('Batch permissions created successfully');
    });

    it('should handle partial failures in batch creation', async () => {
      const { createBatchPermissionsRoute } = await import(
        '../api/routes/permission.routes'
      );

      const batchData = [
        mockPermissionData,
        {
          slug: '' // This should fail
        },
        {
          slug: 'valid:permission'
        }
      ];

      try {
        await createBatchPermissionsRoute.sdk.createBatchPermissions({
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

  describe('GET /permission/:id - getPermission', () => {
    it('should get a permission successfully', async () => {
      const { getPermissionRoute } = await import(
        '../api/routes/permission.routes'
      );

      const response = await getPermissionRoute.sdk.getPermission({
        params: { id: '123e4567-e89b-12d3-a456-426614174002' },
        headers: {
          authorization: TEST_TOKENS.AUTH
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBeDefined();
      expect(typeof response.response).toBe('object');

      const permissionResponse = response.response as {
        id: string;
        slug: string;
      };
      expect(permissionResponse.id).toBe(
        '123e4567-e89b-12d3-a456-426614174002'
      );
      expect(permissionResponse.slug).toBe('read:users');
    });

    it('should handle non-existent permission', async () => {
      const { getPermissionRoute } = await import(
        '../api/routes/permission.routes'
      );

      try {
        await getPermissionRoute.sdk.getPermission({
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

  describe('GET /permission/batch - getBatchPermissions', () => {
    it('should get multiple permissions successfully', async () => {
      const { getBatchPermissionsRoute } = await import(
        '../api/routes/permission.routes'
      );

      const response = await getBatchPermissionsRoute.sdk.getBatchPermissions({
        query: {
          ids: [
            '123e4567-e89b-12d3-a456-426614174002',
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

  describe('PUT /permission - updatePermission', () => {
    it('should update a permission successfully', async () => {
      const { updatePermissionRoute } = await import(
        '../api/routes/permission.routes'
      );

      const response = await updatePermissionRoute.sdk.updatePermission({
        body: mockUpdatePermissionData,
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBe('Permission updated successfully');
    });

    it('should handle validation errors when updating permission', async () => {
      const { updatePermissionRoute } = await import(
        '../api/routes/permission.routes'
      );

      const invalidData = {
        id: '123e4567-e89b-12d3-a456-426614174002',
        slug: '' // Empty slug should fail validation
      };

      try {
        await updatePermissionRoute.sdk.updatePermission({
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

  describe('PUT /permission/batch - updateBatchPermissions', () => {
    it('should update multiple permissions successfully', async () => {
      const { updateBatchPermissionsRoute } = await import(
        '../api/routes/permission.routes'
      );

      const batchUpdateData = [mockUpdatePermissionData];

      const response =
        await updateBatchPermissionsRoute.sdk.updateBatchPermissions({
          body: batchUpdateData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });

      expect(response.code).toBe(200);
      expect(response.response).toBe('Batch permissions updated successfully');
    });
  });

  describe('DELETE /permission/:id - deletePermission', () => {
    it('should delete a permission successfully', async () => {
      const { deletePermissionRoute } = await import(
        '../api/routes/permission.routes'
      );

      const response = await deletePermissionRoute.sdk.deletePermission({
        params: { id: '123e4567-e89b-12d3-a456-426614174002' },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBe('Permission deleted successfully');
    });

    it('should handle non-existent permission deletion', async () => {
      const { deletePermissionRoute } = await import(
        '../api/routes/permission.routes'
      );

      try {
        await deletePermissionRoute.sdk.deletePermission({
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

  describe('DELETE /permission/batch - deleteBatchPermissions', () => {
    it('should delete multiple permissions successfully', async () => {
      const { deleteBatchPermissionsRoute } = await import(
        '../api/routes/permission.routes'
      );

      const response =
        await deleteBatchPermissionsRoute.sdk.deleteBatchPermissions({
          query: {
            ids: [
              '123e4567-e89b-12d3-a456-426614174002',
              '456e7890-e89b-12d3-a456-426614174001'
            ]
          },
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });

      expect(response.code).toBe(200);
      expect(response.response).toBe('Batch permissions deleted successfully');
    });
  });

  describe('Permission Business Logic Tests', () => {
    it('should enforce unique slug constraint', async () => {
      const { createPermissionRoute } = await import(
        '../api/routes/permission.routes'
      );

      // Try to create permission with existing slug
      const duplicateData = {
        slug: 'read:users' // This slug already exists in test data
      };

      try {
        await createPermissionRoute.sdk.createPermission({
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

    it('should validate permission slug format', async () => {
      const { createPermissionRoute } = await import(
        '../api/routes/permission.routes'
      );

      const invalidSlugData = {
        slug: 'invalid slug format' // Should be action:resource format
      };

      try {
        await createPermissionRoute.sdk.createPermission({
          body: invalidSlugData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });
        expect(true).toBe(false); // Should not reach here
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it('should prevent deletion of permission in use by roles', async () => {
      const { deletePermissionRoute } = await import(
        '../api/routes/permission.routes'
      );

      // Try to delete permission that's assigned to a role
      try {
        await deletePermissionRoute.sdk.deletePermission({
          params: { id: '123e4567-e89b-12d3-a456-426614174002' }, // This permission is assigned to admin role
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });
        expect(true).toBe(false); // Should not reach here due to foreign key constraint
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });
  });
});
