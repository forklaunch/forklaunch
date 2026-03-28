import { p, type PropertyBuilders } from '@mikro-orm/core';
import { COMPLIANCE_KEY, type ComplianceLevel } from './complianceTypes';
import { EncryptedType } from './encryptedType';

// ---------------------------------------------------------------------------
// Runtime Proxy implementation
// ---------------------------------------------------------------------------

/**
 * Check whether a value is a MikroORM property builder (has ~options).
 */
function isBuilder(value: unknown): value is object {
  return (
    value != null &&
    typeof value === 'object' &&
    '~options' in (value as Record<string, unknown>)
  );
}

/**
 * Wraps a MikroORM scalar PropertyBuilder in a Proxy that:
 * 1. Adds a `.compliance(level)` method
 * 2. Forwards all other method calls to the underlying builder
 * 3. Re-wraps returned builders so `.compliance()` persists through chains
 */
function wrapUnclassified(builder: unknown): unknown {
  return new Proxy(builder as object, {
    get(target: Record<string | symbol, unknown>, prop) {
      if (prop === 'compliance') {
        return (level: ComplianceLevel) => wrapClassified(target, level);
      }
      if (prop === '~options') return Reflect.get(target, prop, target);
      if (prop === COMPLIANCE_KEY) return undefined;

      const value = Reflect.get(target, prop, target);
      if (typeof value === 'function') {
        return (...args: unknown[]) => {
          const result = (value as (...args: unknown[]) => unknown).apply(
            target,
            args
          );
          return isBuilder(result) ? wrapUnclassified(result) : result;
        };
      }
      return value;
    }
  });
}

/**
 * Compliance levels that require field-level encryption via EncryptedType.
 */
const ENCRYPTED_LEVELS: ReadonlySet<ComplianceLevel> = new Set([
  'pii',
  'phi',
  'pci'
]);

/**
 * Detect the original type hint from the builder's ~options to create
 * the appropriate EncryptedType variant for deserialization.
 */
function detectOriginalType(options: Record<string, unknown>): string {
  const type = options.type;
  if (!type) return 'string';

  // type can be a string name or a Type instance
  let t: string;
  if (typeof type === 'string') {
    t = type.toLowerCase();
  } else if (typeof type === 'object' && type !== null) {
    // Type instance — use constructor name or runtimeType
    const rt = (type as { runtimeType?: string }).runtimeType;
    t = rt ?? type.constructor?.name?.toLowerCase() ?? 'string';
  } else {
    return 'string';
  }

  if (t.includes('json') || t === 'object') return 'json';
  if (
    t.includes('int') ||
    t === 'number' ||
    t === 'double' ||
    t === 'float' ||
    t === 'decimal' ||
    t === 'smallint' ||
    t === 'tinyint'
  )
    return 'number';
  if (t === 'boolean' || t === 'bool') return 'boolean';
  return 'string';
}

/**
 * Wraps a builder that has been classified via `.compliance()`.
 * Stores the compliance level under `~compliance` for `defineComplianceEntity`.
 * For encrypted levels (pii/phi/pci), applies EncryptedType to the builder
 * so MikroORM handles encryption at the data conversion layer.
 * Chaining after `.compliance()` propagates the level through subsequent builders.
 */
function wrapClassified(builder: object, level: ComplianceLevel): unknown {
  // Apply EncryptedType for encrypted compliance levels
  if (ENCRYPTED_LEVELS.has(level)) {
    const options = (builder as Record<string | symbol, unknown>)[
      '~options'
    ] as Record<string, unknown> | undefined;
    if (options) {
      const originalType = detectOriginalType(options);
      options.type = new EncryptedType(originalType);
      // Force column type to text since encrypted output is always a string
      options.columnType = 'text';
      options.runtimeType = originalType === 'json' ? 'object' : originalType;
    }
  }

  return new Proxy(builder, {
    get(target: Record<string | symbol, unknown>, prop) {
      if (prop === COMPLIANCE_KEY) return level;
      if (prop === '~options') return Reflect.get(target, prop, target);

      const value = Reflect.get(target, prop, target);
      if (typeof value === 'function') {
        return (...args: unknown[]) => {
          const result = (value as (...args: unknown[]) => unknown).apply(
            target,
            args
          );
          return isBuilder(result) ? wrapClassified(result, level) : result;
        };
      }
      return value;
    }
  });
}

/**
 * Wraps a relation PropertyBuilder (manyToOne, oneToMany, etc.).
 * Auto-classified as 'none' — no `.compliance()` call needed.
 * All chained methods preserve the auto-classification.
 */
function wrapRelation(builder: object): unknown {
  return wrapClassified(builder, 'none');
}

// ---------------------------------------------------------------------------
// Relation method detection
// ---------------------------------------------------------------------------

const RELATION_METHODS = new Set([
  'manyToOne',
  'oneToMany',
  'manyToMany',
  'oneToOne',
  'embedded'
]);

function isRelationMethod(prop: string | symbol): boolean {
  return typeof prop === 'string' && RELATION_METHODS.has(prop);
}

// ---------------------------------------------------------------------------
// fp — the ForkLaunch property builder
// ---------------------------------------------------------------------------

/**
 * ForkLaunch property builder. Drop-in replacement for MikroORM's `p`
 * that adds `.compliance(level)` to every scalar property builder
 * and auto-classifies relation builders as 'none'.
 *
 * - Scalar fields: `fp.string().compliance('pii')` — must call `.compliance()`
 * - Relation fields: `fp.manyToOne(Target)` — auto-classified, no `.compliance()` needed
 *
 * @example
 * ```typescript
 * import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
 *
 * const User = defineComplianceEntity({
 *   name: 'User',
 *   properties: {
 *     id: fp.uuid().primary().compliance('none'),
 *     email: fp.string().unique().compliance('pii'),
 *     medicalRecord: fp.string().nullable().compliance('phi'),
 *     organization: () => fp.manyToOne(Organization).nullable(),
 *   }
 * });
 * ```
 */
export const fp: PropertyBuilders = new Proxy(p, {
  get(target: Record<string | symbol, unknown>, prop) {
    const value = Reflect.get(target, prop, target);
    if (typeof value !== 'function') return value;

    if (isRelationMethod(prop)) {
      // Relation methods: call the original, wrap result as auto-classified 'none'
      return (...args: unknown[]) => {
        const result = (value as (...args: unknown[]) => unknown).apply(
          target,
          args
        );
        return isBuilder(result) ? wrapRelation(result) : result;
      };
    }

    // Scalar methods: call the original, wrap result with .compliance()
    return (...args: unknown[]) => {
      const result = (value as (...args: unknown[]) => unknown).apply(
        target,
        args
      );
      return isBuilder(result) ? wrapUnclassified(result) : result;
    };
  }
}) as PropertyBuilders;
