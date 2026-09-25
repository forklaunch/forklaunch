import { SchemaValidator } from '@forklaunch/blueprint-core';
import { MapToSdk } from '@forklaunch/core/http';
import {
  eraseUserData,
  exportUserData,
  getDocument,
  listSources,
  search,
  refreshSource,
  assembleTopic,
  getTopic,
  getTopicPhase,
  listTopics
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
  topic: {
    listTopics: typeof listTopics;
    getTopic: typeof getTopic;
    getTopicPhase: typeof getTopicPhase;
    assembleTopic: typeof assembleTopic;
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
  },
  topic: {
    listTopics,
    getTopic,
    getTopicPhase,
    assembleTopic
  }
} satisfies MlseSdk;

export type MlseSdkClient = MapToSdk<SchemaValidator, MlseSdk>;
