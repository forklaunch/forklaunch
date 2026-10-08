import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  surfacePermissions,
  surfaceRoles
} from '../controllers/user.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const userRouter = forklaunchRouter(
  '/user',
  schemaValidator,
  openTelemetryCollector
);

export const surfaceRolesRoute = userRouter.get(
  '/:id/organizations/:organizationId/surface-roles',
  surfaceRoles
);
export const surfacePermissionsRoute = userRouter.get(
  '/:id/organizations/:organizationId/surface-permissions',
  surfacePermissions
);
