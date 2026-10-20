import { SchemaValidator } from '@forklaunch/blueprint-core';
import { MapToSdk } from '@forklaunch/core/http';
import {
  eraseUserData,
  exportUserData,
  surfacePermissions,
  surfaceRoles,
  surfaceScopedPermissions,
  surfaceScopedRoles
} from './api/controllers';

export type IamSdk = {
  compliance: {
    eraseUserData: typeof eraseUserData;
    exportUserData: typeof exportUserData;
  };
  user: {
    surfaceRoles: typeof surfaceRoles;
    surfacePermissions: typeof surfacePermissions;
    surfaceScopedRoles: typeof surfaceScopedRoles;
    surfaceScopedPermissions: typeof surfaceScopedPermissions;
  };
};

export const iamSdkClient = {
  compliance: {
    eraseUserData,
    exportUserData
  },
  user: {
    surfaceRoles,
    surfacePermissions,
    surfaceScopedRoles,
    surfaceScopedPermissions
  }
} satisfies IamSdk;

export type IamSdkClient = MapToSdk<SchemaValidator, IamSdk>;
