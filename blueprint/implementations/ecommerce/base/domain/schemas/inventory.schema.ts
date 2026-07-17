import { serviceSchemaResolver } from '@forklaunch/internal';
import { BaseInventoryServiceSchemas as TypeBoxSchemas } from './typebox/inventory.schema';
import { BaseInventoryServiceSchemas as ZodSchemas } from './zod/inventory.schema';

export const BaseInventoryServiceSchemas = serviceSchemaResolver(
  TypeBoxSchemas,
  ZodSchemas
);
