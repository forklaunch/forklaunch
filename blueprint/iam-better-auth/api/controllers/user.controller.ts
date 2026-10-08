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

export const surfaceRoles = handlers.get(
  schemaValidator,
  '/:id/organizations/:organizationId/surface-roles',
  {
    name: 'Surface User Roles',
    access: 'internal',
    summary:
      'Surfaces the org-scoped roles for a user in the organization bound to their verified token',
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
    params: { ...IdSchema, organizationId: IdSchema.id }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Surfacing user roles', req.params);
    const { id, organizationId } = req.params;

    const role = await serviceFactory().surfaceRole(id, organizationId);

    if (!role) {
      res.status(200).json([]);
      return;
    }

    res.status(200).json([{ name: role }]);
  }
);

export const surfacePermissions = handlers.get(
  schemaValidator,
  '/:id/organizations/:organizationId/surface-permissions',
  {
    name: 'Surface User Permissions',
    access: 'internal',
    summary:
      'Surfaces the org-scoped permissions for a user in the organization bound to their verified token',
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
    params: { ...IdSchema, organizationId: IdSchema.id }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Surfacing user permissions', req.params);
    const { id, organizationId } = req.params;

    const permissions = await serviceFactory().surfacePermissions(
      id,
      organizationId
    );

    res.status(200).json(permissions.map((slug) => ({ slug })));
  }
);
