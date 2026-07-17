import { serviceSchemaResolver } from '@forklaunch/internal';
import { BaseVariantServiceSchemas as TypeBoxSchemas } from './typebox/variant.schema';
import { BaseVariantServiceSchemas as ZodSchemas } from './zod/variant.schema';

export const BaseVariantServiceSchemas = serviceSchemaResolver(
  TypeBoxSchemas,
  ZodSchemas
);
