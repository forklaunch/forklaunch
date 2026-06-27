import {
  defineEntity,
  p,
  type EntityMetadataWithProperties,
  type EntitySchemaWithMeta,
  type InferEntityFromProperties
} from '@mikro-orm/core';
import {
  COMPLIANCE_KEY,
  parseDuration,
  registerEntityCompliance,
  registerEntityRetention,
  registerEntityUserIdField,
  type ComplianceLevel,
  type RetentionPolicy
} from './complianceTypes';

type ValidateProperties<T> = {
  [K in keyof T]: T[K] extends
    | { '~options': { readonly '~c': true } }
    | ((...args: never[]) => unknown)
    ? T[K]
    : { '~options': { readonly '~c': true } };
};

function readComplianceLevel(builder: unknown): ComplianceLevel | undefined {
  if (builder == null || typeof builder !== 'object') return undefined;
  return (builder as Record<string, unknown>)[COMPLIANCE_KEY] as
    | ComplianceLevel
    | undefined;
}

export function defineComplianceEntity<
  const TName extends string,
  const TTableName extends string,
  const TProperties extends Record<string, unknown>,
  const TPK extends (keyof TProperties)[] | undefined = undefined,
  const TBase = never,
  const TRepository = never,
  const TForceObject extends boolean = false
>(
  meta: EntityMetadataWithProperties<
    TName,
    TTableName,
    TProperties & ValidateProperties<TProperties>,
    TPK,
    TBase,
    TRepository,
    TForceObject
  > & { retention?: RetentionPolicy; userIdField?: string }
): EntitySchemaWithMeta<
  TName,
  TTableName,
  InferEntityFromProperties<TProperties, TPK, TBase, TRepository, TForceObject>,
  TBase,
  TProperties
> {
  const entityName = 'name' in meta ? (meta.name as string) : 'Unknown';
  const complianceFields = new Map<string, ComplianceLevel>();

  const rawProperties = meta.properties;
  const resolvedProperties: Record<string, unknown> =
    typeof rawProperties === 'function' ? rawProperties(p) : rawProperties;

  for (const [fieldName, rawProp] of Object.entries(resolvedProperties)) {
    if (typeof rawProp === 'function') {
      // Relations are arrow-wrapped and auto-classified as 'none' — don't
      // call the arrow, since the referenced entity may not be initialized
      // yet (circular reference). MikroORM resolves these lazily.
      complianceFields.set(fieldName, 'none');
      continue;
    }
    const level = readComplianceLevel(rawProp);

    // Default to 'none' if no compliance level is specified
    // This allows defineComplianceEntity to be used with existing entities
    // without requiring immediate compliance classification of all fields
    complianceFields.set(fieldName, level ?? 'none');
  }

  // Handle retention policy validation
  if (meta.retention) {
    parseDuration(meta.retention.duration); // validates at boot — throws if invalid

    if (!resolvedProperties['createdAt']) {
      throw new Error(
        `Entity '${entityName}' has a retention policy but no 'createdAt' property. ` +
          `Retention requires createdAt to compute expiration.`
      );
    }
  }

  // Create EntitySchema using MikroORM's defineEntity
  const schema = defineEntity(
    meta as EntityMetadataWithProperties<
      TName,
      TTableName,
      TProperties & ValidateProperties<TProperties>,
      TPK,
      TBase,
      TRepository,
      TForceObject
    >
  ) as EntitySchemaWithMeta<
    TName,
    TTableName,
    InferEntityFromProperties<
      TProperties,
      TPK,
      TBase,
      TRepository,
      TForceObject
    >,
    TBase,
    TProperties
  >;

  // Store compliance metadata directly in the EntitySchema's meta object
  // This makes it available via ORM's getMetadata() API, eliminating the need
  // for a separate global registry
  schema.meta.compliance = {
    fields: complianceFields,
    userIdField: meta.userIdField,
    retention: meta.retention
  };

  // DEPRECATED: Also write to global registry for backward compatibility
  // This ensures existing code that reads from the registry still works
  registerEntityCompliance(entityName, complianceFields);
  if (meta.retention) {
    registerEntityRetention(entityName, meta.retention);
  }
  if (meta.userIdField) {
    registerEntityUserIdField(entityName, meta.userIdField);
  }

  return schema;
}
