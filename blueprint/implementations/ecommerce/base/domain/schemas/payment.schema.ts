import { serviceSchemaResolver } from '@forklaunch/internal';
import { BasePaymentServiceSchemas as TypeBoxSchemas } from './typebox/payment.schema';
import { BasePaymentServiceSchemas as ZodSchemas } from './zod/payment.schema';

export const BasePaymentServiceSchemas = serviceSchemaResolver(
  TypeBoxSchemas,
  ZodSchemas
);
