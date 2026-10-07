import { SchemaValidator } from '@forklaunch/blueprint-core';
import { MapToSdk } from '@forklaunch/core/http';
import {
  describeCodeSet,
  eraseUserData,
  exportUserData,
  lookupProcedureCode
} from './api/controllers';

export type CacSdk = {
  compliance: {
    eraseUserData: typeof eraseUserData;
    exportUserData: typeof exportUserData;
  };
  codeSet: {
    describeCodeSet: typeof describeCodeSet;
    lookupProcedureCode: typeof lookupProcedureCode;
  };
};

export const cacSdkClient = {
  compliance: {
    eraseUserData,
    exportUserData
  },
  codeSet: {
    describeCodeSet,
    lookupProcedureCode
  }
} satisfies CacSdk;

export type CacSdkClient = MapToSdk<SchemaValidator, CacSdk>;
