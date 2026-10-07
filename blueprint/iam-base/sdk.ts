import { SchemaValidator } from '@forklaunch/blueprint-core';
import { MapToSdk } from '@forklaunch/core/http';
import {
  createBatchPermissions,
  createBatchRoles,
  createBatchUsers,
  createOrganization,
  createPermission,
  createRole,
  createUser,
  deleteBatchPermissions,
  deleteBatchRoles,
  deleteBatchUsers,
  deleteOrganization,
  deletePermission,
  deleteRole,
  deleteUser,
  eraseUserData,
  exportUserData,
  getBatchPermissions,
  getBatchRoles,
  getBatchUsers,
  getOrganization,
  getPermission,
  getRole,
  getUser,
  surfacePermissions,
  surfaceRoles,
  updateBatchPermissions,
  updateBatchRoles,
  updateBatchUsers,
  updateOrganization,
  updatePermission,
  updateRole,
  updateUser
} from './api/controllers';

export type IamSdk = {
  compliance: {
    eraseUserData: typeof eraseUserData;
    exportUserData: typeof exportUserData;
  };
  organization: {
    createOrganization: typeof createOrganization;
    getOrganization: typeof getOrganization;
    updateOrganization: typeof updateOrganization;
    deleteOrganization: typeof deleteOrganization;
  };
  user: {
    createUser: typeof createUser;
    createBatchUsers: typeof createBatchUsers;
    getUser: typeof getUser;
    getBatchUsers: typeof getBatchUsers;
    updateUser: typeof updateUser;
    updateBatchUsers: typeof updateBatchUsers;
    deleteUser: typeof deleteUser;
    deleteBatchUsers: typeof deleteBatchUsers;
    surfaceRoles: typeof surfaceRoles;
    surfacePermissions: typeof surfacePermissions;
  };
  role: {
    createRole: typeof createRole;
    createBatchRoles: typeof createBatchRoles;
    getRole: typeof getRole;
    getBatchRoles: typeof getBatchRoles;
    updateRole: typeof updateRole;
    updateBatchRoles: typeof updateBatchRoles;
    deleteRole: typeof deleteRole;
    deleteBatchRoles: typeof deleteBatchRoles;
  };
  permission: {
    createPermission: typeof createPermission;
    createBatchPermissions: typeof createBatchPermissions;
    getPermission: typeof getPermission;
    getBatchPermissions: typeof getBatchPermissions;
    updatePermission: typeof updatePermission;
    updateBatchPermissions: typeof updateBatchPermissions;
    deletePermission: typeof deletePermission;
    deleteBatchPermissions: typeof deleteBatchPermissions;
  };
};

export const iamSdkClient = {
  compliance: {
    eraseUserData: eraseUserData,
    exportUserData: exportUserData
  },
  organization: {
    createOrganization: createOrganization,
    getOrganization: getOrganization,
    updateOrganization: updateOrganization,
    deleteOrganization: deleteOrganization
  },
  user: {
    createUser: createUser,
    createBatchUsers: createBatchUsers,
    getUser: getUser,
    getBatchUsers: getBatchUsers,
    updateUser: updateUser,
    updateBatchUsers: updateBatchUsers,
    deleteUser: deleteUser,
    deleteBatchUsers: deleteBatchUsers,
    surfaceRoles: surfaceRoles,
    surfacePermissions: surfacePermissions
  },
  role: {
    createRole: createRole,
    createBatchRoles: createBatchRoles,
    getRole: getRole,
    getBatchRoles: getBatchRoles,
    updateRole: updateRole,
    updateBatchRoles: updateBatchRoles,
    deleteRole: deleteRole,
    deleteBatchRoles: deleteBatchRoles
  },
  permission: {
    createPermission: createPermission,
    createBatchPermissions: createBatchPermissions,
    getPermission: getPermission,
    getBatchPermissions: getBatchPermissions,
    updatePermission: updatePermission,
    updateBatchPermissions: updateBatchPermissions,
    deletePermission: deletePermission,
    deleteBatchPermissions: deleteBatchPermissions
  }
} satisfies IamSdk;

export type IamSdkClient = MapToSdk<SchemaValidator, IamSdk>;
