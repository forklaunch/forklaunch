import { serviceSchemaResolver } from '@forklaunch/internal';
import { BaseSmsServiceSchemas as TypeBoxSchemas } from './typebox/sms.schema';
import { BaseSmsServiceSchemas as ZodSchemas } from './zod/sms.schema';

export const BaseSmsServiceSchemas = serviceSchemaResolver(
  TypeBoxSchemas,
  ZodSchemas
);
