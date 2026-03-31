import { safeParse, safeStringify } from '@forklaunch/common';
import { p, Type, type PropertyBuilders } from '@mikro-orm/core';
import { COMPLIANCE_KEY, type ComplianceLevel } from './complianceTypes';
import { EncryptedType, resolveTypeInstance } from './encryptedType';

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
 * Resolve the element runtimeType and isArray flag from builder options.
 * Uses the actual MikroORM Type instance's runtimeType — no manual
 * type-name enumeration needed.
 */
function resolveEncryptedTypeArgs(options: Record<string, unknown>): {
  elementRuntimeType: string;
  isArray: boolean;
} {
  const type = options.type;
  const isArray = options.array === true;

  // For arrays, the type is the element type (constructor or instance).
  // For p.array() with no chained element type, type is an ArrayType instance.
  const resolved = resolveTypeInstance(type);
  if (resolved) {
    const rt = resolved.runtimeType;
    // ArrayType's runtimeType is 'string[]' — treat as array of strings
    if (rt.endsWith('[]')) {
      return { elementRuntimeType: rt.slice(0, -2) || 'string', isArray: true };
    }
    return { elementRuntimeType: rt, isArray };
  }

  // No type set (e.g., plain enum) — default to string
  return { elementRuntimeType: 'string', isArray };
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
      const { elementRuntimeType, isArray } = resolveEncryptedTypeArgs(options);

      // For enum fields, extract allowed values for app-level validation
      // then remove enum metadata so MikroORM won't generate a DB check
      // constraint (the encrypted ciphertext would never satisfy it).
      let enumValues: unknown[] | undefined;
      if (options.items) {
        enumValues = extractEnumValues(options.items);
        // Remove all enum metadata so MikroORM won't generate a DB
        // check constraint (encrypted ciphertext never matches plaintext).
        delete options.items;
        delete options.nativeEnumName;
        delete options.enum;
      }

      options.type = new EncryptedType(elementRuntimeType, isArray, enumValues);
      // Force column type to text since encrypted output is always a string
      options.columnType = 'text';
      options.runtimeType = isArray ? 'object' : elementRuntimeType;
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
// Enum type inference
// ---------------------------------------------------------------------------

/**
 * Extract the raw values from enum items (array, native TS enum object,
 * or lazy factory function `() => EnumObj`).
 */
function extractEnumValues(items: unknown): unknown[] {
  // Resolve lazy enum factory: p.enum(() => MyEnum) stores the function
  if (typeof items === 'function') {
    return extractEnumValues((items as () => unknown)());
  }
  if (Array.isArray(items)) return items;
  if (items != null && typeof items === 'object') {
    // Native TS numeric enums have reverse mappings (value → key).
    // Filter those out by keeping only values whose key is not itself a value.
    const obj = items as Record<string, unknown>;
    const allValues = Object.values(obj);
    return allValues.filter((v) => typeof v !== 'string' || !(v in obj));
  }
  return [];
}

/**
 * MikroORM custom Type that stores mixed-type enum values as text
 * using safeStringify/safeParse for round-trip fidelity.
 */
class EnumTextType extends Type<unknown, string> {
  override convertToDatabaseValue(value: unknown): string {
    return safeStringify(value);
  }

  override convertToJSValue(value: unknown): unknown {
    return safeParse(value);
  }

  override getColumnType(): string {
    return 'text';
  }

  get runtimeType(): string {
    return 'string';
  }
}

/**
 * Infer the MikroORM type for an enum property from its items.
 *
 * - All items are strings → 'text'
 * - All items are numbers → 'integer'
 * - Mixed types → EnumTextType (safeStringify/safeParse round-trip)
 * - No items → 'text'
 */
function inferEnumType(items: unknown): string | EnumTextType {
  const values = extractEnumValues(items);
  if (values.length === 0) return 'text';

  const allStrings = values.every((v) => typeof v === 'string');
  if (allStrings) return 'text';

  const allNumbers = values.every((v) => typeof v === 'number');
  if (allNumbers) return 'integer';

  return new EnumTextType();
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
      if (isBuilder(result)) {
        // For enum(), infer a default type so MikroORM metadata
        // discovery doesn't require an explicit .type() call.
        if (prop === 'enum') {
          const options = (result as Record<string | symbol, unknown>)[
            '~options'
          ] as Record<string, unknown> | undefined;
          if (options && !options.type) {
            options.type = inferEnumType(options.items);
          }
        }
        return wrapUnclassified(result);
      }
      return result;
    };
  }
}) as PropertyBuilders;
