import { SmsStatusEnum } from '@forklaunch/implementation-messaging-base/enum';
import {
  BaseSmsDtos,
  BaseSmsEntities
} from '@forklaunch/implementation-messaging-base/types';

// twilio sms mapper entity/dto types — twilio pins the record status to the
// canonical SmsStatusEnum since provider statuses are mapped onto it
export type TwilioSmsEntities = BaseSmsEntities;
export type TwilioSmsDtos = BaseSmsDtos<typeof SmsStatusEnum>;

// form-encoded payload posted by Twilio delivery-status callbacks
export type TwilioStatusCallbackDto = {
  MessageSid: string;
  MessageStatus: string;
  ErrorMessage?: string;
};

// subset of the Twilio Message resource returned by the Messages REST API
export type TwilioMessageResource = {
  sid: string;
  status: string;
  error_message?: string | null;
};
