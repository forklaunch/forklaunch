import {
  PermissionSchemas,
  RoleSchemas,
  UserSchemas
} from '../../domain/schemas';
import { SchemaValidator } from '@forklaunch/validator/typebox';
const schemaValidator = SchemaValidator();
const { string } = schemaValidator;
const IdSchema = { id: string };
const IdsSchema = { ids: schemaValidator.array(string) };
export const userContract = {
  basePath: '/user',
  createUser: {
    method: 'post',
    path: '/',
    operationId: 'user.createUser',
    details: {
      name: 'Create User',
      access: 'internal',
      summary: 'Creates a new user',
      auth: { scheme: 'hmac' },
      body: UserSchemas.CreateUserSchema,
      responses: {
        201: string,
        500: string
      }
    }
  },
  createBatchUsers: {
    method: 'post',
    path: '/batch',
    operationId: 'user.createBatchUsers',
    details: {
      name: 'Create Batch Users',
      access: 'internal',
      summary: 'Creates multiple users',
      auth: { scheme: 'hmac' },
      body: schemaValidator.array(UserSchemas.CreateUserSchema),
      responses: {
        201: string,
        500: string
      }
    }
  },
  getUser: {
    method: 'get',
    path: '/:id',
    operationId: 'user.getUser',
    details: {
      name: 'Get User',
      access: 'protected',
      summary: 'Gets a user by ID',
      auth: {
        sessionSchema: {
          organizationId: string
        },
        scheme: 'jwt',
        allowedPermissions: ['platform:read']
      },
      responses: {
        200: UserSchemas.UserSchema,
        500: string
      },
      params: IdSchema
    }
  },
  getBatchUsers: {
    method: 'get',
    path: '/batch',
    operationId: 'user.getBatchUsers',
    details: {
      name: 'Get Batch Users',
      access: 'protected',
      summary: 'Gets multiple users by IDs',
      auth: {
        sessionSchema: {
          organizationId: string
        },
        scheme: 'jwt',
        allowedPermissions: ['platform:read']
      },
      responses: {
        200: schemaValidator.array(UserSchemas.UserSchema),
        500: string
      },
      query: IdsSchema
    }
  },
  updateUser: {
    method: 'put',
    path: '/',
    operationId: 'user.updateUser',
    details: {
      name: 'Update User',
      access: 'protected',
      summary: 'Updates a user by ID',
      auth: {
        sessionSchema: {
          organizationId: string
        },
        scheme: 'jwt',
        allowedRoles: ['platform:read']
      },
      body: UserSchemas.UpdateUserSchema,
      responses: {
        200: string,
        401: string,
        403: string,
        404: string,
        500: string
      }
    }
  },
  updateBatchUsers: {
    method: 'put',
    path: '/batch',
    operationId: 'user.updateBatchUsers',
    details: {
      name: 'Update Batch Users',
      access: 'protected',
      summary: 'Updates multiple users by IDs',
      auth: {
        sessionSchema: {
          organizationId: string
        },
        scheme: 'jwt',
        allowedRoles: ['platform:read']
      },
      body: schemaValidator.array(UserSchemas.UpdateUserSchema),
      responses: {
        200: string,
        401: string,
        403: string,
        404: string,
        500: string
      }
    }
  },
  deleteUser: {
    method: 'delete',
    path: '/:id',
    operationId: 'user.deleteUser',
    details: {
      name: 'Delete User',
      access: 'protected',
      summary: 'Deletes a user by ID',
      auth: {
        sessionSchema: {
          organizationId: string
        },
        scheme: 'jwt',
        allowedRoles: ['platform:read']
      },
      responses: {
        200: string,
        401: string,
        403: string,
        404: string,
        500: string
      },
      params: IdSchema
    }
  },
  deleteBatchUsers: {
    method: 'delete',
    path: '/batch',
    operationId: 'user.deleteBatchUsers',
    details: {
      name: 'Delete Batch Users',
      access: 'protected',
      summary: 'Deletes multiple users by IDs',
      auth: {
        sessionSchema: {
          organizationId: string
        },
        scheme: 'jwt',
        allowedRoles: ['platform:read']
      },
      responses: {
        200: string,
        401: string,
        403: string,
        404: string,
        500: string
      },
      query: IdsSchema
    }
  },
  surfaceRoles: {
    method: 'get',
    path: '/:id/surface-roles',
    operationId: 'user.surfaceRoles',
    details: {
      name: 'Surface User Roles',
      access: 'internal',
      summary: 'Surfaces the roles for a user',
      auth: { scheme: 'hmac' },
      responses: {
        200: schemaValidator.array(RoleSchemas.RoleSchema),
        500: string
      },
      params: {
        ...IdSchema
      }
    }
  },
  surfacePermissions: {
    method: 'get',
    path: '/:id/surface-permissions',
    operationId: 'user.surfacePermissions',
    details: {
      name: 'Surface User Permissions',
      access: 'internal',
      summary: 'Verifies if a user has a specified permission',
      auth: { scheme: 'hmac' },
      responses: {
        200: schemaValidator.array(PermissionSchemas.PermissionSchema),
        500: string
      },
      params: {
        ...IdSchema
      }
    }
  }
} as const;
