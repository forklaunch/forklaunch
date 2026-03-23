import { defineEntity } from '@mikro-orm/core';
import type { InferEntity } from '@mikro-orm/core';
import {
  COMPLIANCE_KEY,
  type ClassifiedProperty,
  type ComplianceLevel,
  registerEntityCompliance
} from './complianceTypes';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/**
 * Maps each property to `never` if it doesn't extend ClassifiedProperty.
 * Used in an intersection with TProperties to produce a type error
 * when any property is not classified.
 */
type AssertAllClassified<T extends Record<string, unknown>> = {
  [K in keyof T]: T[K] extends ClassifiedProperty
    ? T[K]
    : T[K] extends () => ClassifiedProperty
      ? T[K]
      : ClassifiedProperty; // Force error: value not assignable to ClassifiedProperty
};

/**
 * Metadata descriptor for `defineComplianceEntity`.
 */
interface ComplianceEntityMetadata<
  TProperties extends Record<string, unknown>
> {
  name: string;
  tableName?: string;
  properties: TProperties & AssertAllClassified<TProperties>;
  extends?: unknown;
  primaryKeys?: string[];
  hooks?: Record<string, unknown>;
  repository?: () => unknown;
  forceObject?: boolean;
  inheritance?: 'tpt';
  orderBy?: Record<string, unknown> | Record<string, unknown>[];
  discriminatorColumn?: string;
  versionProperty?: string;
  concurrencyCheckKeys?: Set<string>;
  serializedPrimaryKey?: string;
  indexes?: unknown[];
  uniques?: unknown[];
}

// ---------------------------------------------------------------------------
// Runtime helpers
// ---------------------------------------------------------------------------

/**
 * Read the compliance level from a proxy-wrapped builder.
 * Returns the level if the proxy has `~compliance`, undefined otherwise.
 */
function readComplianceLevel(builder: unknown): ComplianceLevel | undefined {
  if (builder == null || typeof builder !== 'object') return undefined;
  return (builder as Record<string, unknown>)[COMPLIANCE_KEY] as
    | ComplianceLevel
    | undefined;
}

// ---------------------------------------------------------------------------
// defineComplianceEntity
// ---------------------------------------------------------------------------

/**
 * Wrapper around MikroORM's `defineEntity` that enforces compliance
 * classification on every field.
 *
 * - Scalar fields: must call `.compliance(level)` — forgetting it is a
 *   compile-time error (TypeScript rejects it) AND a runtime error.
 * - Relation fields: auto-classified as `'none'` by the `fp` builder.
 *
 * Compliance metadata is stored in a module-level registry, accessible via
 * `getComplianceMetadata(entityName, fieldName)`.
 *
 * @example
 * ```typescript
 * const User = defineComplianceEntity({
 *   name: 'User',
 *   properties: {
 *     id: fp.uuid().primary().compliance('none'),
 *     email: fp.string().unique().compliance('pii'),
 *     medicalRecord: fp.string().nullable().compliance('phi'),
 *     organization: () => fp.manyToOne(Organization).nullable(),
 *   }
 * });
 * export type User = InferEntity<typeof User>;
 * ```
 */
export function defineComplianceEntity<
  TProperties extends Record<string, unknown>
>(meta: ComplianceEntityMetadata<TProperties>) {
  const { name: entityName, properties, ...rest } = meta;
  const complianceFields = new Map<string, ComplianceLevel>();

  // Validate and extract compliance from each property
  for (const [fieldName, rawProp] of Object.entries(properties)) {
    const prop = typeof rawProp === 'function' ? rawProp() : rawProp;
    const level = readComplianceLevel(prop);

    if (level == null) {
      throw new Error(
        `Field '${entityName}.${fieldName}' is missing compliance classification. ` +
          `Call .compliance('pii' | 'phi' | 'pci' | 'none') on this property, ` +
          `or use a relation method (fp.manyToOne, etc.) which is auto-classified.`
      );
    }
    complianceFields.set(fieldName, level);
  }

  // Store compliance metadata in the global registry
  registerEntityCompliance(entityName, complianceFields);

  // Delegate to MikroORM's defineEntity.
  // The Proxy-wrapped builders forward ~options correctly.
  return defineEntity({
    name: entityName,
    properties: properties as Record<string, unknown>,
    ...rest
  } as Parameters<typeof defineEntity>[0]);
}

// Re-export InferEntity for convenience
export type { InferEntity };
