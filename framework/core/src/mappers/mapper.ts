import { Prettify } from '@forklaunch/common';
import {
  AnySchemaValidator,
  IdiomaticSchema,
  prettyPrintParseErrors,
  Schema,
  SchemaValidator
} from '@forklaunch/validator';
import { EntitySchema } from '@mikro-orm/core';

export function requestMapper<
  SV extends AnySchemaValidator,
  DomainSchema extends IdiomaticSchema<SV>,
  Entity,
  AdditionalArgs extends unknown[] = []
>({
  schemaValidator,
  schema,
  entity,
  mapperDefinition
}: {
  schemaValidator: SV;
  schema: DomainSchema;
  entity: EntitySchema<Entity>;
  mapperDefinition: {
    toEntity: (
      dto: Schema<DomainSchema, SV>,
      ...args: AdditionalArgs
    ) => Promise<Entity>;
  };
}): {
  entity: EntitySchema<Entity>;
  schema: DomainSchema;
} & typeof mapperDefinition {
  const sv = schemaValidator as SchemaValidator;
  return {
    ...mapperDefinition,
    entity,
    schema,

    toEntity: async (
      dto: Schema<DomainSchema, SV>,
      ...args: AdditionalArgs
    ) => {
      const parsedSchema = sv.parse(sv.schemify(schema), dto);
      if (!parsedSchema.ok) {
        throw new Error(prettyPrintParseErrors(parsedSchema.errors, 'DTO'));
      }
      return mapperDefinition.toEntity(
        dto as Schema<DomainSchema, SV>,
        ...(args as AdditionalArgs)
      );
    }
  };
}

export function responseMapper<
  SV extends AnySchemaValidator,
  DomainSchema extends IdiomaticSchema<SV>,
  Entity,
  AdditionalArgs extends unknown[] = []
>({
  schemaValidator,
  schema,
  entity,
  mapperDefinition
}: {
  schemaValidator: SV;
  schema: DomainSchema;
  entity: EntitySchema<Entity>;
  mapperDefinition: {
    toDto: (
      entity: Entity,
      ...args: AdditionalArgs
    ) => Promise<Schema<DomainSchema, SV>>;
  };
}): Prettify<
  {
    entity: EntitySchema<Entity>;
    schema: DomainSchema;
  } & typeof mapperDefinition
> {
  const sv = schemaValidator as SchemaValidator;
  return {
    ...mapperDefinition,
    entity,
    schema,

    toDto: async (entity: Entity, ...args: AdditionalArgs) => {
      const domain = await mapperDefinition.toDto(entity, ...args);
      const parsedSchema = sv.parse(sv.schemify(schema), domain);
      if (!parsedSchema.ok) {
        throw new Error(prettyPrintParseErrors(parsedSchema.errors, 'DTO'));
      }
      return domain;
    }
  };
}
