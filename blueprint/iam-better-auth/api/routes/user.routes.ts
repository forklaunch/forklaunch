import { userContract } from '../contracts/user.contract';
import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  surfacePermissions,
  surfaceRoles,
  surfaceScopedPermissions,
  surfaceScopedRoles
} from '../controllers/user.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const userRouter = forklaunchRouter(
  userContract.basePath,
  schemaValidator,
  openTelemetryCollector
);

export const surfaceRolesRoute = userRouter.get(
  userContract.surfaceRoles.path,
  surfaceRoles
);
export const surfacePermissionsRoute = userRouter.get(
  userContract.surfacePermissions.path,
  surfacePermissions
);

export const surfaceScopedRolesRoute = userRouter.get(
  userContract.surfaceScopedRoles.path,
  surfaceScopedRoles
);
export const surfaceScopedPermissionsRoute = userRouter.get(
  userContract.surfaceScopedPermissions.path,
  surfaceScopedPermissions
);
