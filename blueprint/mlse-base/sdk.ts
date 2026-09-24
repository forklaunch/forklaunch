import { SchemaValidator } from '@forklaunch/blueprint-core';
import { MapToSdk } from '@forklaunch/core/http';
import {
  eraseUserData,
  exportUserData,
  listSources
} from './api/controllers';

export type MlseSdk = {
  compliance: {
    eraseUserData: typeof eraseUserData;
    exportUserData: typeof exportUserData;
  };
  source: {
    listSources: typeof listSources;
  };
};

export const mlseSdkClient = {
  compliance: {
    eraseUserData,
    exportUserData
  },
  source: {
    listSources
  }
} satisfies MlseSdk;

export type MlseSdkClient = MapToSdk<SchemaValidator, MlseSdk>;
