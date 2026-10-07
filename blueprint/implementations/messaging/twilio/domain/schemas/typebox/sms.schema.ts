import { SmsStatusEnum } from '@forklaunch/implementation-messaging-base/enum';
import {
  date,
  enum_,
  optional,
  record,
  string,
  unknown,
  uuid
} from '@forklaunch/validator/typebox';

export const SendSmsSchema = {
  to: string,
  body: string,
  metadata: optional(record(string, unknown))
};

export const SmsRecordSchema = ({ uuidId }: { uuidId: boolean }) => ({
  id: uuidId ? uuid : string,
  to: string,
  body: string,
  status: enum_(SmsStatusEnum),
  providerMessageId: optional(string),
  error: optional(string),
  metadata: optional(record(string, unknown)),
  createdAt: optional(date),
  updatedAt: optional(date)
});

export const TwilioSmsServiceSchemas = (options: { uuidId: boolean }) => ({
  SendSmsSchema,
  SmsRecordSchema: SmsRecordSchema(options)
});
