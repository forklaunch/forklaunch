import { SchemaValidator } from '@forklaunch/blueprint-core';
import { MapToSdk } from '@forklaunch/core/http';
import {
  answer,
  answerComplete,
  approveTopic,
  createContentLicense,
  deleteSavedSearch,
  flagContent,
  listContentFlags,
  listContentLicenses,
  listSavedSearches,
  listSourceAccess,
  listVoiceSettings,
  registerLicensedSource,
  resolveContentFlag,
  revokeContentLicense,
  saveSearch,
  searchHistory,
  setVoiceSetting,
  voiceQuery,
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
  admin: {
    listSourceAccess: typeof listSourceAccess;
    registerLicensedSource: typeof registerLicensedSource;
    listContentLicenses: typeof listContentLicenses;
    createContentLicense: typeof createContentLicense;
    revokeContentLicense: typeof revokeContentLicense;
    flagContent: typeof flagContent;
    listContentFlags: typeof listContentFlags;
    resolveContentFlag: typeof resolveContentFlag;
    approveTopic: typeof approveTopic;
  };
  savedSearch: {
    listSavedSearches: typeof listSavedSearches;
    saveSearch: typeof saveSearch;
    deleteSavedSearch: typeof deleteSavedSearch;
    searchHistory: typeof searchHistory;
  };
  voice: {
    voiceQuery: typeof voiceQuery;
    listVoiceSettings: typeof listVoiceSettings;
    setVoiceSetting: typeof setVoiceSetting;
  };
  answer: {
    answer: typeof answer;
    answerComplete: typeof answerComplete;
  };
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
  admin: {
    listSourceAccess,
    registerLicensedSource,
    listContentLicenses,
    createContentLicense,
    revokeContentLicense,
    flagContent,
    listContentFlags,
    resolveContentFlag,
    approveTopic
  },
  savedSearch: {
    listSavedSearches,
    saveSearch,
    deleteSavedSearch,
    searchHistory
  },
  voice: {
    voiceQuery,
    listVoiceSettings,
    setVoiceSetting
  },
  answer: {
    answer,
    answerComplete
  },
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
