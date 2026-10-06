import {
  defineEntity,
  p,
  UnderscoreNamingStrategy,
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
import {
  CompliantIndexType,
  installCompliantAccessors,
  isCompliantType,
  type CompliantPropertyBinding
} from './compliantField';

const naming = new UnderscoreNamingStrategy();

/**
 * Wire compliant (pii/phi/pci) properties: name their specs, give queryable
 * ones a blind-index column plus a hidden envelope column, and collect the
 * accessor bindings to install on the entity class.
 *
 * A queryable property keeps its original column for the envelope (so
 * existing ciphertext stays where it is) and moves the property itself to
 * `<column>_idx`, which is what `where` compares against.
 */
function wireCompliantProperties(
  entityName: string,
  properties: Record<string, unknown>
): {
  properties: Record<string, unknown>;
  bindings: CompliantPropertyBinding[];
} {
  const wired: Record<string, unknown> = {};
  const bindings: CompliantPropertyBinding[] = [];
  for (const [name, prop] of Object.entries(properties)) {
    wired[name] = prop;
    if (typeof prop === 'function' || prop == null) continue;
    const options = ((prop as Record<string, unknown>)['~options'] ??
      prop) as Record<string, unknown>;
    const type = options.type;
    if (!isCompliantType(type)) continue;
    type.spec.path = `${entityName}.${name}`;
    if (!(type instanceof CompliantIndexType)) {
      bindings.push({ property: name, spec: type.spec });
      continue;
    }
    const sibling = `${name}Sealed`;
    if (sibling in properties) {
      throw new Error(
        `Entity '${entityName}': '${sibling}' is reserved for the encrypted value of queryable field '${name}'`
      );
    }
    const explicit =
      options.fieldName ?? (options.fieldNames as string[] | undefined)?.[0];
    const column =
      typeof explicit === 'string'
        ? explicit
        : naming.propertyToColumnName(name);
    const required = options.nullable !== true;
    options.fieldName = `${column}_idx`;
    delete options.fieldNames;
    // The index column is always nullable: adding it to a populated table
    // must not fail, and legacy rows have no index until the sweep writes
    // one. Required-ness is enforced by the envelope column below.
    options.nullable = true;
    // Declared right after the property: hydration must write the index
    // before the envelope (see compliantField.ts writeProperty).
    wired[sibling] = {
      type: 'text',
      columnType: 'text',
      fieldName: column,
      nullable: !required,
      hidden: true,
      accessor: true
    };
    bindings.push({ property: name, spec: type.spec, sibling });
  }
  return { properties: wired, bindings };
}

// Accepts any classification level rather than a bare `true`, now that
// `.compliance()` records which one was chosen. The requirement is unchanged:
// every property must have been classified.
type ValidateProperties<T> = {
  [K in keyof T]: T[K] extends
    | { '~options': { readonly '~c': ComplianceLevel } }
    | ((...args: never[]) => unknown)
    ? T[K]
    : { '~options': { readonly '~c': ComplianceLevel } };
};

function readComplianceLevel(builder: unknown): ComplianceLevel | undefined {
  if (builder == null || typeof builder !== 'object') return undefined;
  return (builder as Record<string, unknown>)[COMPLIANCE_KEY] as
    ComplianceLevel | undefined;
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

  const wired = wireCompliantProperties(entityName, resolvedProperties);
  const schema = defineEntity({
    ...meta,
    properties: wired.properties
  } as unknown as EntityMetadataWithProperties<
    TName,
    TTableName,
    TProperties & ValidateProperties<TProperties>,
    TPK,
    TBase,
    TRepository,
    TForceObject
  >);

  if (wired.bindings.length > 0) {
    const entityClass = schema.meta.class as { prototype: object } | undefined;
    if (entityClass)
      installCompliantAccessors(entityClass.prototype, wired.bindings);
    // A class swapped in later (setClass) needs the accessors too.
    const setClass = schema.setClass.bind(schema);
    schema.setClass = ((cls: { prototype: object }) => {
      installCompliantAccessors(cls.prototype, wired.bindings);
      return setClass(cls as never);
    }) as typeof schema.setClass;
  }

  return schema as unknown as EntitySchemaWithMeta<
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
