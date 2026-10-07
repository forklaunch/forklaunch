import { LiteralSchema } from '@forklaunch/validator';
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

export const SmsRecordSchema =
  ({ uuidId }: { uuidId: boolean }) =>
  <T extends Record<string, LiteralSchema>>(StatusEnum: T) => ({
    id: uuidId ? uuid : string,
    to: string,
    body: string,
    status: enum_(StatusEnum),
    providerMessageId: optional(string),
    error: optional(string),
    metadata: optional(record(string, unknown)),
    createdAt: optional(date),
    updatedAt: optional(date)
  });

export const BaseSmsServiceSchemas = (options: { uuidId: boolean }) => ({
  SendSmsSchema,
  SmsRecordSchema: SmsRecordSchema(options)
});
