import type {
  PropertyBuilders,
  PropertyChain,
  UniversalPropertyOptionsBuilder
} from '@mikro-orm/core';

/**
 * Classification levels for entity field compliance.
 * Drives encryption (phi/pci), audit log redaction (all non-none), and compliance reporting.
 */
export const ComplianceLevel = {
  pii: 'pii',
  phi: 'phi',
  pci: 'pci',
  none: 'none'
} as const;
export type ComplianceLevel =
  (typeof ComplianceLevel)[keyof typeof ComplianceLevel];

/**
 * Brand symbol — makes ClassifiedProperty structurally distinct from
 * plain PropertyChain at the TypeScript level.
 */
declare const CLASSIFIED: unique symbol;

/**
 * A property that has been classified via `.compliance()`.
 * Only ClassifiedProperty values are accepted by `defineComplianceEntity`.
 *
 * At runtime this is a Proxy wrapping a MikroORM PropertyBuilder.
 * The brand exists only at the type level for compile-time enforcement.
 */
export interface ClassifiedProperty {
  readonly [CLASSIFIED]: true;
}

/**
 * Internal key used by the runtime Proxy to store compliance level
 * on the builder instance. Not part of the public API.
 */
export const COMPLIANCE_KEY = '~compliance' as const;

// ---------------------------------------------------------------------------
// Compliance metadata registry
// ---------------------------------------------------------------------------

/** entityName → (fieldName → ComplianceLevel) */
const complianceRegistry = new Map<string, Map<string, ComplianceLevel>>();

/**
 * Register compliance metadata for an entity's fields.
 * Called by `defineComplianceEntity` during entity definition.
 */
export function registerEntityCompliance(
  entityName: string,
  fields: Map<string, ComplianceLevel>
): void {
  complianceRegistry.set(entityName, fields);
}

/**
 * Look up the compliance level for a single field on an entity.
 * Returns `'none'` if the entity or field is not registered.
 */
export function getComplianceMetadata(
  entityName: string,
  fieldName: string
): ComplianceLevel {
  return complianceRegistry.get(entityName)?.get(fieldName) ?? 'none';
}

/**
 * Get all compliance fields for an entity.
 * Returns undefined if the entity is not registered.
 */
export function getEntityComplianceFields(
  entityName: string
): Map<string, ComplianceLevel> | undefined {
  return complianceRegistry.get(entityName);
}

/**
 * Check whether an entity has any fields requiring encryption (phi or pci).
 */
export function entityHasEncryptedFields(entityName: string): boolean {
  const fields = complianceRegistry.get(entityName);
  if (!fields) return false;
  for (const level of fields.values()) {
    if (level === 'phi' || level === 'pci') return true;
  }
  return false;
}

// ---------------------------------------------------------------------------
// ForklaunchPropertyChain — remapped PropertyChain that preserves
// `.compliance()` through method chaining.
// ---------------------------------------------------------------------------

/**
 * Recursively remaps every method on PropertyChain<V,O> so those returning
 * PropertyChain<V2,O2> instead return ForklaunchPropertyChain<V2,O2>.
 * This preserves the `.compliance()` method through chained calls like
 * `.nullable().unique()`.
 */

export interface ForklaunchPropertyChain<Value, Options> extends RemapReturns<
  Value,
  Options
> {
  /**
   * Classify this field's compliance level. Must be called on every scalar
   * field passed to `defineComplianceEntity`.
   * Returns an opaque `ClassifiedProperty`.
   */
  compliance(level: ComplianceLevel): ClassifiedProperty;
}

type RemapReturns<Value, Options> = {
  [K in keyof PropertyChain<Value, Options>]: PropertyChain<
    Value,
    Options
  >[K] extends (...args: infer A) => PropertyChain<infer V2, infer O2>
    ? (...args: A) => ForklaunchPropertyChain<V2, O2>
    : PropertyChain<Value, Options>[K];
};

// ---------------------------------------------------------------------------
// ForklaunchPropertyBuilders — the type of `fp`
// ---------------------------------------------------------------------------

/**
 * Keys on PropertyBuilders that return relation builders.
 * These are auto-classified as 'none' — the fp proxy wraps them
 * to return ClassifiedProperty directly.
 */
type RelationBuilderKeys =
  | 'manyToOne'
  | 'oneToMany'
  | 'manyToMany'
  | 'oneToOne'
  | 'embedded';

/**
 * The type of `fp` — mirrors `PropertyBuilders` but:
 * - Scalar methods return `ForklaunchPropertyChain` (must call `.compliance()`)
 * - Relation methods return `ClassifiedProperty` directly (auto 'none')
 */
export type ForklaunchPropertyBuilders = {
  [K in Exclude<
    keyof PropertyBuilders,
    RelationBuilderKeys
  >]: PropertyBuilders[K] extends (
    ...args: infer A
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
  ) => UniversalPropertyOptionsBuilder<infer V, infer O, infer _IK>
    ? (...args: A) => ForklaunchPropertyChain<V, O>
    : PropertyBuilders[K] extends (
          ...args: infer A
        ) => PropertyChain<infer V, infer O>
      ? (...args: A) => ForklaunchPropertyChain<V, O>
      : PropertyBuilders[K];
} & {
  [K in RelationBuilderKeys]: PropertyBuilders[K] extends (
    ...args: infer A
  ) => PropertyChain<infer V, infer O>
    ? (...args: A) => ClassifiedRelationChain<V, O>
    : PropertyBuilders[K];
};

/**
 * A relation builder that is already classified (as 'none') but still
 * supports chaining relation-specific methods like `.mappedBy()`, `.nullable()`.
 * All chain methods return ClassifiedRelationChain (preserving the brand).
 */
export type ClassifiedRelationChain<Value, Options> = {
  [K in keyof PropertyChain<Value, Options>]: PropertyChain<
    Value,
    Options
  >[K] extends (...args: infer A) => PropertyChain<infer V2, infer O2>
    ? (...args: A) => ClassifiedRelationChain<V2, O2>
    : PropertyChain<Value, Options>[K];
} & ClassifiedProperty;
