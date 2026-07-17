import { SchemaValidator } from '../../schema';
import { Schema } from '@forklaunch/validator';
import { Variant } from '../../persistence/entities';
import {
  CreateVariantMapper,
  UpdateVariantMapper,
  VariantMapper
} from '../mappers/variant.mappers';

// variant
export type VariantMapperTypes = {
  VariantMapper: typeof Variant;
  CreateVariantMapper: typeof Variant;
  UpdateVariantMapper: typeof Variant;
};
export type VariantDtoTypes = {
  VariantMapper: Schema<typeof VariantMapper.schema, SchemaValidator>;
  CreateVariantMapper: Schema<typeof CreateVariantMapper.schema, SchemaValidator>;
  UpdateVariantMapper: Schema<typeof UpdateVariantMapper.schema, SchemaValidator>;
};

// Remaining entities' mapper/DTO types are added incrementally as each PR lands.
