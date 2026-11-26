import { roleContract } from '../contracts/role.contract';
import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  createBatchRoles,
  createRole,
  deleteBatchRoles,
  deleteRole,
  getBatchRoles,
  getRole,
  updateBatchRoles,
  updateRole
} from '../controllers/role.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const roleRouter = forklaunchRouter(
  roleContract.basePath,
  schemaValidator,
  openTelemetryCollector
);

export const createRoleRoute = roleRouter.post(
  roleContract.createRole.path,
  createRole
);
export const createBatchRolesRoute = roleRouter.post(
  roleContract.createBatchRoles.path,
  createBatchRoles
);
export const getBatchRolesRoute = roleRouter.get(
  roleContract.getBatchRoles.path,
  getBatchRoles
);
export const getRoleRoute = roleRouter.get(roleContract.getRole.path, getRole);
export const updateRoleRoute = roleRouter.put(
  roleContract.updateRole.path,
  updateRole
);
export const updateBatchRolesRoute = roleRouter.put(
  roleContract.updateBatchRoles.path,
  updateBatchRoles
);
export const deleteBatchRolesRoute = roleRouter.delete(
  roleContract.deleteBatchRoles.path,
  deleteBatchRoles
);
export const deleteRoleRoute = roleRouter.delete(
  roleContract.deleteRole.path,
  deleteRole
);
