import { serviceSchemaResolver } from '@forklaunch/internal';
import { BaseCartServiceSchemas as TypeBoxSchemas } from './typebox/cart.schema';
import { BaseCartServiceSchemas as ZodSchemas } from './zod/cart.schema';

export const BaseCartServiceSchemas = serviceSchemaResolver(
  TypeBoxSchemas,
  ZodSchemas
);
