import {
  array,
  handlers,
  IdSchema,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);
const serviceFactory = ci.scopedResolver(tokens.SurfacingService);

// User-only endpoints cannot choose a session or organization securely.
// Retained for fail-closed old clients; upgraded helpers use scoped endpoints.

export const surfaceRoles = handlers.get(
  schemaValidator,
  '/:id/surface-roles',
  {
    name: 'Surface User Roles',
    access: 'internal',
    summary:
      'Retired user-only lookup; returns no roles until the client is upgraded',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    responses: {
      200: array({ name: string }),
      500: string
    },
    params: IdSchema
  },
  async (req, res) => {
    openTelemetryCollector.debug('Surfacing user roles', req.params);

    res.status(200).json([]);
  }
);

export const surfacePermissions = handlers.get(
  schemaValidator,
  '/:id/surface-permissions',
  {
    name: 'Surface User Permissions',
    access: 'internal',
    summary:
      'Retired user-only lookup; returns no permissions until the client is upgraded',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    responses: {
      200: array({ slug: string }),
      500: string
    },
    params: IdSchema
  },
  async (req, res) => {
    openTelemetryCollector.debug('Surfacing user permissions', req.params);

    res.status(200).json([]);
  }
);

export const surfaceScopedRoles = handlers.get(
  schemaValidator,
  '/:id/organizations/:organizationId/sessions/:sessionId/surface-roles',
  {
    name: 'Surface Scoped User Roles',
    access: 'internal',
    summary:
      'Surface current membership for the exact authenticated session and organization',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    responses: { 200: array({ name: string }), 500: string },
    params: { ...IdSchema, organizationId: string, sessionId: string }
  },
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
  '/:id/organizations/:organizationId/sessions/:sessionId/surface-permissions',
  {
    name: 'Surface Scoped User Permissions',
    access: 'internal',
    summary:
      'Surface current permissions for the exact authenticated session and organization',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    responses: { 200: array({ slug: string }), 500: string },
    params: { ...IdSchema, organizationId: string, sessionId: string }
  },
  async (req, res) => {
    const permissions = await serviceFactory().surfacePermissions({
      userId: req.params.id,
      organizationId: req.params.organizationId,
      sessionId: req.params.sessionId
    });
    res.status(200).json(permissions.map((slug) => ({ slug })));
  }
);
