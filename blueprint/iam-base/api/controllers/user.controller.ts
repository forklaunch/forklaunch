import {
  array,
  handlers,
  IdSchema,
  IdsSchema,
  PLATFORM_READ_PERMISSIONS,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { IdsDto } from '@forklaunch/common';
import { getCachedJwks } from '@forklaunch/core/http';
import { jwtVerify } from 'jose';
import { ci, tokens } from '../../bootstrapper';
import { PermissionMapper } from '../../domain/mappers/permission.mappers';
import { RoleMapper } from '../../domain/mappers/role.mappers';
import {
  CreateUserMapper,
  UpdateUserMapper,
  UserMapper
} from '../../domain/mappers/user.mappers';

const openTelemetryCollector = ci.resolve(tokens.OpenTelemetryCollector);
const serviceFactory = ci.scopedResolver(tokens.UserService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);
const JWKS_PUBLIC_KEY_URL = ci.resolve(tokens.JWKS_PUBLIC_KEY_URL);

const decodeResourceWithOrganizationId = async (token: string) => {
  const jwks = await getCachedJwks(JWKS_PUBLIC_KEY_URL);
  for (const jwk of jwks) {
    const { payload } = await jwtVerify(token, jwk);
    if (payload.sub) {
      const organizationId = await serviceFactory().getOrganizationIdByUserId({
        id: payload.sub
      });

      if (!organizationId) {
        throw new Error('User not found or has no organization');
      }

      return {
        ...payload,
        organizationId
      };
    }
  }

  throw new Error('User ID not found or JWKS not valid');
};

export const createUser = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'Create User',
    access: 'internal',
    summary: 'Creates a new user',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    body: CreateUserMapper.schema,
    responses: {
      201: string,
      500: string
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Creating user', req.body);

    await serviceFactory().createUser(req.body);
    res.status(201).send('User created successfully');
  }
);

export const createBatchUsers = handlers.post(
  schemaValidator,
  '/batch',
  {
    name: 'Create Batch Users',
    access: 'internal',
    summary: 'Creates multiple users',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    body: array(CreateUserMapper.schema),
    responses: {
      201: string,
      500: string
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Creating batch users', req.body);
    await serviceFactory().createBatchUsers(req.body);
    res.status(201).send('Batch users created successfully');
  }
);

export const getUser = handlers.get(
  schemaValidator,
  '/:id',
  {
    name: 'Get User',
    access: 'protected',
    summary: 'Gets a user by ID',
    auth: {
      sessionSchema: {
        organizationId: string
      },
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      decodeResource: decodeResourceWithOrganizationId,
      allowedPermissions: PLATFORM_READ_PERMISSIONS
    },
    responses: {
      200: UserMapper.schema,
      500: string
    },
    params: IdSchema
  },
  async (req, res) => {
    if (!req.session?.organizationId) {
      return res.status(401).send('Unauthorized: missing organization context');
    }

    const user = await serviceFactory().getUser({
      id: req.params.id,
      organization: {
        id: req.session.organizationId
      }
    });
    openTelemetryCollector.debug('Retrieving user', req.params);
    res.status(200).json(user);
  }
);

export const getBatchUsers = handlers.get(
  schemaValidator,
  '/batch',
  {
    name: 'Get Batch Users',
    access: 'protected',
    summary: 'Gets multiple users by IDs',
    auth: {
      sessionSchema: {
        organizationId: string
      },
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      decodeResource: decodeResourceWithOrganizationId,
      allowedPermissions: PLATFORM_READ_PERMISSIONS
    },
    responses: {
      200: array(UserMapper.schema),
      500: string
    },
    query: IdsSchema
  },
  async (req, res) => {
    openTelemetryCollector.debug('Retrieving batch users', req.query);

    if (!req.session?.organizationId) {
      return res.status(401).send('Unauthorized: missing organization context');
    }

    res.status(200).json(
      await serviceFactory().getBatchUsers({
        ...req.query,
        organization: {
          id: req.session.organizationId
        }
      } as IdsDto)
    );
  }
);

export const updateUser = handlers.put(
  schemaValidator,
  '/',
  {
    name: 'Update User',
    access: 'protected',
    summary: 'Updates a user by ID',
    auth: {
      sessionSchema: {
        organizationId: string
      },
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      decodeResource: decodeResourceWithOrganizationId,
      allowedRoles: PLATFORM_READ_PERMISSIONS
    },
    body: UpdateUserMapper.schema,
    responses: {
      200: string,
      401: string,
      403: string,
      404: string,
      500: string
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Updating user', req.body);

    if (!req.session?.organizationId) {
      return res.status(401).send('Unauthorized: missing organization context');
    }

    const targetUserOrgId = await serviceFactory().getOrganizationIdByUserId({
      id: req.body.id
    });

    if (!targetUserOrgId) {
      return res.status(404).send('User not found or has no organization');
    }

    if (targetUserOrgId !== req.session.organizationId) {
      return res
        .status(403)
        .send('Forbidden: cannot update user from different organization');
    }

    await serviceFactory().updateUser(req.body);
    res.status(200).send('User updated successfully');
  }
);

export const updateBatchUsers = handlers.put(
  schemaValidator,
  '/batch',
  {
    name: 'Update Batch Users',
    access: 'protected',
    summary: 'Updates multiple users by IDs',
    auth: {
      sessionSchema: {
        organizationId: string
      },
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      decodeResource: decodeResourceWithOrganizationId,
      allowedRoles: PLATFORM_READ_PERMISSIONS
    },
    body: array(UpdateUserMapper.schema),
    responses: {
      200: string,
      401: string,
      403: string,
      404: string,
      500: string
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Updating batch users', req.body);

    if (!req.session?.organizationId) {
      return res.status(401).send('Unauthorized: missing organization context');
    }

    for (const userDto of req.body) {
      const targetUserOrgId = await serviceFactory().getOrganizationIdByUserId({
        id: userDto.id
      });

      if (!targetUserOrgId) {
        return res.status(404).send('User not found or has no organization');
      }

      if (targetUserOrgId !== req.session.organizationId) {
        return res
          .status(403)
          .send('Forbidden: cannot update user from different organization');
      }
    }

    await serviceFactory().updateBatchUsers(req.body);
    res.status(200).send('Batch users updated successfully');
  }
);

export const deleteUser = handlers.delete(
  schemaValidator,
  '/:id',
  {
    name: 'Delete User',
    access: 'protected',
    summary: 'Deletes a user by ID',
    auth: {
      sessionSchema: {
        organizationId: string
      },
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      decodeResource: decodeResourceWithOrganizationId,
      allowedRoles: PLATFORM_READ_PERMISSIONS
    },
    responses: {
      200: string,
      401: string,
      403: string,
      404: string,
      500: string
    },
    params: IdSchema
  },
  async (req, res) => {
    openTelemetryCollector.debug('Deleting user', req.params);

    if (!req.session?.organizationId) {
      return res.status(401).send('Unauthorized: missing organization context');
    }

    const targetUserOrgId = await serviceFactory().getOrganizationIdByUserId({
      id: req.params.id
    });

    if (!targetUserOrgId) {
      return res.status(404).send('User not found or has no organization');
    }

    if (targetUserOrgId !== req.session.organizationId) {
      return res
        .status(403)
        .send('Forbidden: cannot delete user from different organization');
    }

    await serviceFactory().deleteUser({
      id: req.params.id,
      organization: {
        id: req.session.organizationId
      }
    });
    res.status(200).send('User deleted successfully');
  }
);

export const deleteBatchUsers = handlers.delete(
  schemaValidator,
  '/batch',
  {
    name: 'Delete Batch Users',
    access: 'protected',
    summary: 'Deletes multiple users by IDs',
    auth: {
      sessionSchema: {
        organizationId: string
      },
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      decodeResource: decodeResourceWithOrganizationId,
      allowedRoles: PLATFORM_READ_PERMISSIONS
    },
    responses: {
      200: string,
      401: string,
      403: string,
      404: string,
      500: string
    },
    query: IdsSchema
  },
  async (req, res) => {
    openTelemetryCollector.debug('Deleting batch users', req.query);

    if (!req.session?.organizationId) {
      return res.status(401).send('Unauthorized: missing organization context');
    }

    for (const userId of req.query.ids) {
      const targetUserOrgId = await serviceFactory().getOrganizationIdByUserId({
        id: userId
      });

      if (!targetUserOrgId) {
        return res.status(404).send('User not found or has no organization');
      }

      if (targetUserOrgId !== req.session.organizationId) {
        return res
          .status(403)
          .send('Forbidden: cannot delete user from different organization');
      }
    }

    // Pass organization constraint to service layer
    await serviceFactory().deleteBatchUsers({
      ...req.query,
      organization: {
        id: req.session.organizationId
      }
    });
    res.status(200).send('Batch users deleted successfully');
  }
);

/**
 * Surface User Roles
 * @param req - The request object
 * @param res - The response object
 * @returns The permissions for the user
 *
 * Leverages the HMAC authentication method to verify the user's permissions.
 *
 * To create the HMAC token, use the createHmacToken function:
 * ```typescript
 * createHmacToken({
 *   method: 'GET',
 *   path: '/123414/surface-roles',
 *   body?: {
 *     someKey: '123414'
 *   },
 *   timestamp: '1234567890',
 *   nonce: '1234567890',
 *   secretKey: 'test'
 * });
 * ```
 */
export const surfaceRoles = handlers.get(
  schemaValidator,
  '/:id/surface-roles',
  {
    name: 'Surface User Roles',
    access: 'internal',
    summary: 'Surfaces the roles for a user',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    responses: {
      200: array(RoleMapper.schema),
      500: string
    },
    params: {
      ...IdSchema
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Verifying user role', req.params);
    const { id } = req.params;
    const roles = await serviceFactory().surfaceRoles({ id });
    res.status(200).json(roles);
  }
);

/**
 * Surface User Permissions
 * @param req - The request object
 * @param res - The response object
 * @returns The permissions for the user
 *
 * Leverages the HMAC authentication method to verify the user's permissions.
 *
 * To create the HMAC token, use the createHmacToken function:
 * ```typescript
 * createHmacToken({
 *   method: 'GET',
 *   path: '/123414/surface-permissions',
 *   body?: {
 *     someKey: '123414'
 *   },
 *   timestamp: '1234567890',
 *   nonce: '1234567890',
 *   secretKey: 'test'
 * });
 * ```
 */
export const surfacePermissions = handlers.get(
  schemaValidator,
  '/:id/surface-permissions',
  {
    name: 'Surface User Permissions',
    access: 'internal',
    summary: 'Verifies if a user has a specified permission',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    responses: {
      200: array(PermissionMapper.schema),
      500: string
    },
    params: {
      ...IdSchema
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Verifying user permission', req.params);
    const { id } = req.params;
    const permissions = await serviceFactory().surfacePermissions({ id });
    res.status(200).json(permissions);
  }
);
