import { SchemaValidator } from '@forklaunch/blueprint-core';
import { MapToSdk } from '@forklaunch/core/http';
import { surfacePermissions, surfaceRoles } from './api/controllers';

export type IamSdk = {
  user: {
    surfaceRoles: typeof surfaceRoles;
    surfacePermissions: typeof surfacePermissions;
  };
};

export const iamSdkClient = {
  user: {
    surfaceRoles,
    surfacePermissions
  }
} satisfies IamSdk;

export type IamSdkClient = MapToSdk<SchemaValidator, IamSdk>;
