import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  createBatchPermissions,
  createPermission,
  deleteBatchPermissions,
  deletePermission,
  getBatchPermissions,
  getPermission,
  updateBatchPermissions,
  updatePermission
} from '../controllers/permission.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const permissionRouter = forklaunchRouter(
  '/permission',
  schemaValidator,
  openTelemetryCollector
);

export const createPermissionRoute = permissionRouter.post(
  '/',
  createPermission
);
export const createBatchPermissionsRoute = permissionRouter.post(
  '/batch',
  createBatchPermissions
);
export const getBatchPermissionsRoute = permissionRouter.get(
  '/batch',
  getBatchPermissions
);
export const getPermissionRoute = permissionRouter.get('/:id', getPermission);
export const updatePermissionRoute = permissionRouter.put(
  '/',
  updatePermission
);
export const updateBatchPermissionsRoute = permissionRouter.put(
  '/batch',
  updateBatchPermissions
);
export const deleteBatchPermissionsRoute = permissionRouter.delete(
  '/batch',
  deleteBatchPermissions
);
export const deletePermissionRoute = permissionRouter.delete(
  '/:id',
  deletePermission
);
