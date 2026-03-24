import {
  array,
  handlers,
  IdSchema,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OpenTelemetryCollector);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);
const serviceFactory = ci.scopedResolver(tokens.SurfacingService);

export const surfaceRoles = handlers.get(
  schemaValidator,
  '/:id/surface-roles',
  {
    name: 'Surface User Roles',
    access: 'internal',
    summary:
      'Surfaces the org-scoped roles for a user in their active organization',
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
    const { id } = req.params;

    const role = await serviceFactory().surfaceRole(id);

    if (!role) {
      res.status(200).json([]);
      return;
    }

    res.status(200).json([{ name: role }]);
  }
);

export const surfacePermissions = handlers.get(
  schemaValidator,
  '/:id/surface-permissions',
  {
    name: 'Surface User Permissions',
    access: 'internal',
    summary:
      'Surfaces the org-scoped permissions for a user in their active organization',
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
    const { id } = req.params;

    const permissions = await serviceFactory().surfacePermissions(id);

    res.status(200).json(permissions.map((slug) => ({ slug })));
  }
);
