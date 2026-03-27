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

    if (level == null) {
      throw new Error(
        `Field '${entityName}.${fieldName}' is missing compliance classification. ` +
          `Call .compliance('pii' | 'phi' | 'pci' | 'none') on this property, ` +
          `or use a relation method (fp.manyToOne, etc.) which is auto-classified.`
      );
    }
    complianceFields.set(fieldName, level);
  }

  registerEntityCompliance(entityName, complianceFields);

  // Handle retention policy
  if (meta.retention) {
    parseDuration(meta.retention.duration); // validates at boot — throws if invalid

    if (!resolvedProperties['createdAt']) {
      throw new Error(
        `Entity '${entityName}' has a retention policy but no 'createdAt' property. ` +
          `Retention requires createdAt to compute expiration.`
      );
    }

    registerEntityRetention(entityName, meta.retention);
  }

  // Register userIdField (defaults to 'userId' if not specified)
  if (meta.userIdField) {
    registerEntityUserIdField(entityName, meta.userIdField);
  }

  return defineEntity(
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
}
