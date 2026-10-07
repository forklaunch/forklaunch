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
  '/role',
  schemaValidator,
  openTelemetryCollector
);

export const createRoleRoute = roleRouter.post('/', createRole);
export const createBatchRolesRoute = roleRouter.post(
  '/batch',
  createBatchRoles
);
export const getBatchRolesRoute = roleRouter.get('/batch', getBatchRoles);
export const getRoleRoute = roleRouter.get('/:id', getRole);
export const updateRoleRoute = roleRouter.put('/', updateRole);
export const updateBatchRolesRoute = roleRouter.put('/batch', updateBatchRoles);
export const deleteBatchRolesRoute = roleRouter.delete(
  '/batch',
  deleteBatchRoles
);
export const deleteRoleRoute = roleRouter.delete('/:id', deleteRole);
