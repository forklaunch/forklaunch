import { serviceSchemaResolver } from '@forklaunch/internal';
import { BaseProductServiceSchemas as TypeBoxSchemas } from './typebox/product.schema';
import { BaseProductServiceSchemas as ZodSchemas } from './zod/product.schema';

export const BaseProductServiceSchemas = serviceSchemaResolver(
  TypeBoxSchemas,
  ZodSchemas
);
