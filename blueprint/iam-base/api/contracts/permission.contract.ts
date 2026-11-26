import { PermissionSchemas } from '../../domain/schemas';
import { SchemaValidator } from '@forklaunch/validator/typebox';
const schemaValidator = SchemaValidator();
const { string } = schemaValidator;
const IdSchema = { id: string };
const IdsSchema = { ids: schemaValidator.array(string) };
export const permissionContract = {
  basePath: '/permission',
  createPermission: {
    method: 'post',
    path: '/',
    operationId: 'permission.createPermission',
    details: {
      name: 'Create Permission',
      access: 'internal',
      summary: 'Creates a new permission',
      auth: { scheme: 'hmac' },
      body: PermissionSchemas.CreatePermissionSchema,
      responses: {
        201: string,
        500: string
      }
    }
  },
  createBatchPermissions: {
    method: 'post',
    path: '/batch',
    operationId: 'permission.createBatchPermissions',
    details: {
      name: 'Create Batch Permissions',
      access: 'internal',
      summary: 'Creates multiple permissions',
      auth: { scheme: 'hmac' },
      body: schemaValidator.array(PermissionSchemas.CreatePermissionSchema),
      responses: {
        201: string,
        500: string
      }
    }
  },
  getPermission: {
    method: 'get',
    path: '/:id',
    operationId: 'permission.getPermission',
    details: {
      name: 'Get Permission',
      access: 'protected',
      summary: 'Gets a permission by ID',
      auth: {
        scheme: 'jwt',
        allowedPermissions: ['platform:read']
      },
      responses: {
        200: PermissionSchemas.PermissionSchema,
        500: string
      },
      params: IdSchema
    }
  },
  getBatchPermissions: {
    method: 'get',
    path: '/batch',
    operationId: 'permission.getBatchPermissions',
    details: {
      name: 'Get Batch Permissions',
      access: 'protected',
      summary: 'Gets multiple permissions by IDs',
      auth: {
        scheme: 'jwt',
        allowedPermissions: ['platform:read']
      },
      responses: {
        200: schemaValidator.array(PermissionSchemas.PermissionSchema),
        500: string
      },
      query: IdsSchema
    }
  },
  updatePermission: {
    method: 'put',
    path: '/',
    operationId: 'permission.updatePermission',
    details: {
      name: 'Update Permission',
      access: 'internal',
      summary: 'Updates a permission by ID',
      auth: { scheme: 'hmac' },
      body: PermissionSchemas.UpdatePermissionSchema,
      responses: {
        200: string,
        500: string
      }
    }
  },
  updateBatchPermissions: {
    method: 'put',
    path: '/batch',
    operationId: 'permission.updateBatchPermissions',
    details: {
      name: 'Update Batch Permissions',
      access: 'internal',
      summary: 'Updates multiple permissions by IDs',
      auth: { scheme: 'hmac' },
      body: schemaValidator.array(PermissionSchemas.UpdatePermissionSchema),
      responses: {
        200: string,
        500: string
      }
    }
  },
  deletePermission: {
    method: 'delete',
    path: '/:id',
    operationId: 'permission.deletePermission',
    details: {
      name: 'Delete Permission',
      access: 'internal',
      summary: 'Deletes a permission by ID',
      auth: { scheme: 'hmac' },
      responses: {
        200: string,
        500: string
      },
      params: IdSchema
    }
  },
  deleteBatchPermissions: {
    method: 'delete',
    path: '/batch',
    operationId: 'permission.deleteBatchPermissions',
    details: {
      name: 'Delete Batch Permissions',
      access: 'internal',
      summary: 'Deletes multiple permissions by IDs',
      auth: { scheme: 'hmac' },
      responses: {
        200: string,
        500: string
      },
      query: IdsSchema
    }
  }
} as const;
