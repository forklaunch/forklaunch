import { SchemaValidator } from '@forklaunch/blueprint-core';
import { MapToSdk } from '@forklaunch/core/http';
import {
  eraseUserData,
  exportUserData,
  getSmsRecord,
  sendSms
} from './api/controllers';

export type MessagingSdk = {
  compliance: {
    eraseUserData: typeof eraseUserData;
    exportUserData: typeof exportUserData;
  };
  sms: {
    sendSms: typeof sendSms;
    getSmsRecord: typeof getSmsRecord;
  };
};

export const messagingSdkClient = {
  compliance: {
    eraseUserData,
    exportUserData
  },
  sms: {
    sendSms: sendSms,
    getSmsRecord: getSmsRecord
  }
} satisfies MessagingSdk;

export type MessagingSdkClient = MapToSdk<SchemaValidator, MessagingSdk>;
