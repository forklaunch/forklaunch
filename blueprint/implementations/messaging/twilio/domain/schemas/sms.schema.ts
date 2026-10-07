import { serviceSchemaResolver } from '@forklaunch/internal';
import { TwilioSmsServiceSchemas as TypeBoxSchemas } from './typebox/sms.schema';
import { TwilioSmsServiceSchemas as ZodSchemas } from './zod/sms.schema';

export const TwilioSmsServiceSchemas = serviceSchemaResolver(
  TypeBoxSchemas,
  ZodSchemas
);
