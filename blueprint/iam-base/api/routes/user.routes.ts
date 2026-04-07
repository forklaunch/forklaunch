import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  createBatchUsers,
  createUser,
  deleteBatchUsers,
  deleteUser,
  getBatchUsers,
  getUser,
  surfacePermissions,
  surfaceRoles,
  updateBatchUsers,
  updateUser
} from '../controllers/user.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const userRouter = forklaunchRouter(
  '/user',
  schemaValidator,
  openTelemetryCollector
);

export const createUserRoute = userRouter.post('/', createUser);
export const createBatchUsersRoute = userRouter.post(
  '/batch',
  createBatchUsers
);
export const getBatchUsersRoute = userRouter.get('/batch', getBatchUsers);
export const surfaceRolesRoute = userRouter.get(
  '/:id/surface-roles',
  surfaceRoles
);
export const surfacePermissionsRoute = userRouter.get(
  '/:id/surface-permissions',
  surfacePermissions
);
export const getUserRoute = userRouter.get('/:id', getUser);
export const updateUserRoute = userRouter.put('/', updateUser);
export const updateBatchUsersRoute = userRouter.put('/batch', updateBatchUsers);
export const deleteBatchUsersRoute = userRouter.delete(
  '/batch',
  deleteBatchUsers
);
export const deleteUserRoute = userRouter.delete('/:id', deleteUser);
