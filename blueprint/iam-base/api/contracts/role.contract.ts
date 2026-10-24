import { RoleSchemas } from '../../domain/schemas';
import { SchemaValidator } from '@forklaunch/validator/typebox';
const schemaValidator = SchemaValidator();
const { string } = schemaValidator;
const IdSchema = { id: string };
const IdsSchema = { ids: schemaValidator.array(string) };
export const roleContract = {
  basePath: '/role',
  createRole: {
    method: 'post',
    path: '/',
    operationId: 'role.createRole',
    details: {
      name: 'Create Role',
      access: 'internal',
      summary: 'Creates a new role',
      auth: { scheme: 'hmac' },
      body: RoleSchemas.CreateRoleSchema,
      responses: {
        201: string,
        500: string
      }
    }
  },
  createBatchRoles: {
    method: 'post',
    path: '/batch',
    operationId: 'role.createBatchRoles',
    details: {
      name: 'Create Batch Roles',
      access: 'internal',
      summary: 'Creates multiple roles',
      auth: { scheme: 'hmac' },
      body: schemaValidator.array(RoleSchemas.CreateRoleSchema),
      responses: {
        201: string,
        500: string
      }
    }
  },
  getRole: {
    method: 'get',
    path: '/:id',
    operationId: 'role.getRole',
    details: {
      name: 'Get Role',
      access: 'protected',
      summary: 'Gets a role by ID',
      auth: {
        scheme: 'jwt',
        allowedPermissions: ['platform:read']
      },
      responses: {
        200: RoleSchemas.RoleSchema,
        500: string
      },
      params: IdSchema
    }
  },
  getBatchRoles: {
    method: 'get',
    path: '/batch',
    operationId: 'role.getBatchRoles',
    details: {
      name: 'Get Batch Roles',
      access: 'protected',
      summary: 'Gets multiple roles by IDs',
      auth: {
        scheme: 'jwt',
        allowedPermissions: ['platform:read']
      },
      responses: {
        200: schemaValidator.array(RoleSchemas.RoleSchema),
        500: string
      },
      query: IdsSchema
    }
  },
  updateRole: {
    method: 'put',
    path: '/',
    operationId: 'role.updateRole',
    details: {
      name: 'Update Role',
      access: 'internal',
      summary: 'Updates a role by ID',
      auth: { scheme: 'hmac' },
      body: RoleSchemas.UpdateRoleSchema,
      responses: {
        200: string,
        500: string
      }
    }
  },
  updateBatchRoles: {
    method: 'put',
    path: '/batch',
    operationId: 'role.updateBatchRoles',
    details: {
      name: 'Update Batch Roles',
      access: 'internal',
      summary: 'Updates multiple roles by IDs',
      auth: { scheme: 'hmac' },
      body: schemaValidator.array(RoleSchemas.UpdateRoleSchema),
      responses: {
        200: string,
        500: string
      }
    }
  },
  deleteRole: {
    method: 'delete',
    path: '/:id',
    operationId: 'role.deleteRole',
    details: {
      name: 'Delete Role',
      access: 'internal',
      summary: 'Deletes a role by ID',
      auth: { scheme: 'hmac' },
      responses: {
        200: string,
        500: string
      },
      params: IdSchema
    }
  },
  deleteBatchRoles: {
    method: 'delete',
    path: '/batch',
    operationId: 'role.deleteBatchRoles',
    details: {
      name: 'Delete Batch Roles',
      access: 'internal',
      summary: 'Deletes multiple roles by IDs',
      auth: { scheme: 'hmac' },
      responses: {
        200: string,
        500: string
      },
      query: IdsSchema
    }
  }
} as const;
