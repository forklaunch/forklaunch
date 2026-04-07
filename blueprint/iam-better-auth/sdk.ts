import { SchemaValidator } from '@forklaunch/blueprint-core';
import { MapToSdk } from '@forklaunch/core/http';
import {
  eraseUserData,
  exportUserData,
  surfacePermissions,
  surfaceRoles
} from './api/controllers';

export type IamSdk = {
  compliance: {
    eraseUserData: typeof eraseUserData;
    exportUserData: typeof exportUserData;
  };
  user: {
    surfaceRoles: typeof surfaceRoles;
    surfacePermissions: typeof surfacePermissions;
  };
};

export const iamSdkClient = {
  compliance: {
    eraseUserData,
    exportUserData
  },
  user: {
    surfaceRoles,
    surfacePermissions
  }
} satisfies IamSdk;

export type IamSdkClient = MapToSdk<SchemaValidator, IamSdk>;
