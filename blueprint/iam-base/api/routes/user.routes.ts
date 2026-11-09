import { userContract } from '../contracts/user.contract';
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
  userContract.basePath,
  schemaValidator,
  openTelemetryCollector
);

export const createUserRoute = userRouter.post(
  userContract.createUser.path,
  createUser
);
export const createBatchUsersRoute = userRouter.post(
  userContract.createBatchUsers.path,
  createBatchUsers
);
export const getBatchUsersRoute = userRouter.get(
  userContract.getBatchUsers.path,
  getBatchUsers
);
export const surfaceRolesRoute = userRouter.get(
  userContract.surfaceRoles.path,
  surfaceRoles
);
export const surfacePermissionsRoute = userRouter.get(
  userContract.surfacePermissions.path,
  surfacePermissions
);
export const getUserRoute = userRouter.get(userContract.getUser.path, getUser);
export const updateUserRoute = userRouter.put(
  userContract.updateUser.path,
  updateUser
);
export const updateBatchUsersRoute = userRouter.put(
  userContract.updateBatchUsers.path,
  updateBatchUsers
);
export const deleteBatchUsersRoute = userRouter.delete(
  userContract.deleteBatchUsers.path,
  deleteBatchUsers
);
export const deleteUserRoute = userRouter.delete(
  userContract.deleteUser.path,
  deleteUser
);
