import { bindRuntimeAuth } from '@forklaunch/blueprint-core';
import { userContract } from '../contracts/user.contract';
import { handlers, schemaValidator } from '@forklaunch/blueprint-core';
import { IdsDto } from '@forklaunch/common';
import { getCachedJwks } from '@forklaunch/core/http';
import { jwtVerify } from 'jose';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
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
  userContract.createUser.path,
  bindRuntimeAuth(userContract.createUser.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Creating user', req.body);

    await serviceFactory().createUser(req.body);
    res.status(201).send('User created successfully');
  }
);

export const createBatchUsers = handlers.post(
  schemaValidator,
  userContract.createBatchUsers.path,
  bindRuntimeAuth(userContract.createBatchUsers.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Creating batch users', req.body);
    await serviceFactory().createBatchUsers(req.body);
    res.status(201).send('Batch users created successfully');
  }
);

export const getUser = handlers.get(
  schemaValidator,
  userContract.getUser.path,
  bindRuntimeAuth(userContract.getUser.details, {
    jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL,
    decodeResource: decodeResourceWithOrganizationId
  }),
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
  userContract.getBatchUsers.path,
  bindRuntimeAuth(userContract.getBatchUsers.details, {
    jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL,
    decodeResource: decodeResourceWithOrganizationId
  }),
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
  userContract.updateUser.path,
  bindRuntimeAuth(userContract.updateUser.details, {
    jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL,
    decodeResource: decodeResourceWithOrganizationId
  }),
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
  userContract.updateBatchUsers.path,
  bindRuntimeAuth(userContract.updateBatchUsers.details, {
    jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL,
    decodeResource: decodeResourceWithOrganizationId
  }),
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
  userContract.deleteUser.path,
  bindRuntimeAuth(userContract.deleteUser.details, {
    jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL,
    decodeResource: decodeResourceWithOrganizationId
  }),
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
  userContract.deleteBatchUsers.path,
  bindRuntimeAuth(userContract.deleteBatchUsers.details, {
    jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL,
    decodeResource: decodeResourceWithOrganizationId
  }),
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
 *   path: '/user/123414/surface-roles',
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
  userContract.surfaceRoles.path,
  bindRuntimeAuth(userContract.surfaceRoles.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
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
 *   path: '/user/123414/surface-permissions',
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
  userContract.surfacePermissions.path,
  bindRuntimeAuth(userContract.surfacePermissions.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Verifying user permission', req.params);
    const { id } = req.params;
    const permissions = await serviceFactory().surfacePermissions({ id });
    res.status(200).json(permissions);
  }
);
