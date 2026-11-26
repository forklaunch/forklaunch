import { permissionContract } from '../contracts/permission.contract';
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
  permissionContract.basePath,
  schemaValidator,
  openTelemetryCollector
);

export const createPermissionRoute = permissionRouter.post(
  permissionContract.createPermission.path,
  createPermission
);
export const createBatchPermissionsRoute = permissionRouter.post(
  permissionContract.createBatchPermissions.path,
  createBatchPermissions
);
export const getBatchPermissionsRoute = permissionRouter.get(
  permissionContract.getBatchPermissions.path,
  getBatchPermissions
);
export const getPermissionRoute = permissionRouter.get(
  permissionContract.getPermission.path,
  getPermission
);
export const updatePermissionRoute = permissionRouter.put(
  permissionContract.updatePermission.path,
  updatePermission
);
export const updateBatchPermissionsRoute = permissionRouter.put(
  permissionContract.updateBatchPermissions.path,
  updateBatchPermissions
);
export const deleteBatchPermissionsRoute = permissionRouter.delete(
  permissionContract.deleteBatchPermissions.path,
  deleteBatchPermissions
);
export const deletePermissionRoute = permissionRouter.delete(
  permissionContract.deletePermission.path,
  deletePermission
);
