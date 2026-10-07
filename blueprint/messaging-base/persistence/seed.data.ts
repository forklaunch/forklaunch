import { SmsStatusEnum } from '@forklaunch/implementation-messaging-base/enum';

//! Begin seed data
export const smsRecord = {
  to: '+15555550100',
  body: 'Welcome to forklaunch!',
  status: SmsStatusEnum.SENT,
  providerMessageId: null,
  error: null,
  metadata: null
};
