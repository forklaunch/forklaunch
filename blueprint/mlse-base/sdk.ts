import { SchemaValidator } from '@forklaunch/blueprint-core';
import { MapToSdk } from '@forklaunch/core/http';
import {
  eraseUserData,
  exportUserData,
  getDocument,
  listSources,
  search,
  refreshSource
} from './api/controllers';

export type MlseSdk = {
  compliance: {
    eraseUserData: typeof eraseUserData;
    exportUserData: typeof exportUserData;
  };
  document: {
    getDocument: typeof getDocument;
  };
  search: {
    search: typeof search;
  };
  source: {
    listSources: typeof listSources;
    refreshSource: typeof refreshSource;
  };
};

export const mlseSdkClient = {
  compliance: {
    eraseUserData,
    exportUserData
  },
  document: {
    getDocument
  },
  search: {
    search
  },
  source: {
    listSources,
    refreshSource
  }
} satisfies MlseSdk;

export type MlseSdkClient = MapToSdk<SchemaValidator, MlseSdk>;
