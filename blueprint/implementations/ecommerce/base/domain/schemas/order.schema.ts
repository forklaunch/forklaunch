import { serviceSchemaResolver } from '@forklaunch/internal';
import { BaseOrderServiceSchemas as TypeBoxSchemas } from './typebox/order.schema';
import { BaseOrderServiceSchemas as ZodSchemas } from './zod/order.schema';

export const BaseOrderServiceSchemas = serviceSchemaResolver(
  TypeBoxSchemas,
  ZodSchemas
);
