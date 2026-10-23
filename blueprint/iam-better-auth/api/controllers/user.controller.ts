import { bindRuntimeAuth } from '@forklaunch/blueprint-core';
import { userContract } from '../contracts/user.contract';
import { handlers, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);
const serviceFactory = ci.scopedResolver(tokens.SurfacingService);

// User-only endpoints cannot choose a session or organization securely.
// Retained for fail-closed old clients; upgraded helpers use scoped endpoints.

export const surfaceRoles = handlers.get(
  schemaValidator,
  userContract.surfaceRoles.path,
  bindRuntimeAuth(userContract.surfaceRoles.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Surfacing user roles', req.params);

    res.status(200).json([]);
  }
);

export const surfacePermissions = handlers.get(
  schemaValidator,
  userContract.surfacePermissions.path,
  bindRuntimeAuth(userContract.surfacePermissions.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Surfacing user permissions', req.params);

    res.status(200).json([]);
  }
);

export const surfaceScopedRoles = handlers.get(
  schemaValidator,
  userContract.surfaceScopedRoles.path,
  bindRuntimeAuth(userContract.surfaceScopedRoles.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
  async (req, res) => {
    const role = await serviceFactory().surfaceRole({
      userId: req.params.id,
      organizationId: req.params.organizationId,
      sessionId: req.params.sessionId
    });
    res.status(200).json(role ? [{ name: role }] : []);
  }
);

export const surfaceScopedPermissions = handlers.get(
  schemaValidator,
  userContract.surfaceScopedPermissions.path,
  bindRuntimeAuth(userContract.surfaceScopedPermissions.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
  async (req, res) => {
    const permissions = await serviceFactory().surfacePermissions({
      userId: req.params.id,
      organizationId: req.params.organizationId,
      sessionId: req.params.sessionId
    });
    res.status(200).json(permissions.map((slug) => ({ slug })));
  }
);
