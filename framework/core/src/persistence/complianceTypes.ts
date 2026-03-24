import type {
  EmptyOptions,
  PropertyBuilders,
  PropertyChain
} from '@mikro-orm/core';

export const ComplianceLevel = {
  pii: 'pii',
  phi: 'phi',
  pci: 'pci',
  none: 'none'
} as const;
export type ComplianceLevel =
  (typeof ComplianceLevel)[keyof typeof ComplianceLevel];

export const COMPLIANCE_KEY = '~compliance' as const;

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

const complianceRegistry = new Map<string, Map<string, ComplianceLevel>>();

export function registerEntityCompliance(
  entityName: string,
  fields: Map<string, ComplianceLevel>
): void {
  complianceRegistry.set(entityName, fields);
}

export function getComplianceMetadata(
  entityName: string,
  fieldName: string
): ComplianceLevel {
  return complianceRegistry.get(entityName)?.get(fieldName) ?? 'none';
}

export function getEntityComplianceFields(
  entityName: string
): Map<string, ComplianceLevel> | undefined {
  return complianceRegistry.get(entityName);
}

export function entityHasEncryptedFields(entityName: string): boolean {
  const fields = complianceRegistry.get(entityName);
  if (!fields) return false;
  for (const level of fields.values()) {
    if (level === 'phi' || level === 'pci') return true;
  }
  return false;
}

// ---------------------------------------------------------------------------
// ClassifiedProperty — tagged wrapper: inner builder + compliance level
// ---------------------------------------------------------------------------

/**
 * Tagged wrapper returned by `.compliance()`.
 * `__inner` is the EXACT MikroORM builder type.
 * `defineComplianceEntity` checks for this wrapper and extracts `__inner`.
 */
export interface ClassifiedProperty<
  Builder = unknown,
  Value = unknown,
  Options = unknown
> {
  readonly __inner: Builder;
  readonly __compliance: ComplianceLevel;
  readonly '~type'?: { value: Value };
  readonly '~options': Options;
}

// ---------------------------------------------------------------------------
// ExtractInner — unwraps ClassifiedProperty to get the raw builder
// ---------------------------------------------------------------------------

/**
 * For each property, if it's a ClassifiedProperty, extract __inner.
 * Functions (lazy relations) and raw PropertyChain (relations) pass through.
 */
export type ExtractInner<T> = {
  [K in keyof T]: T[K] extends ClassifiedProperty<infer B> ? B : T[K];
};

// ---------------------------------------------------------------------------
// WithCompliance — adds .compliance() to any MikroORM builder
// ---------------------------------------------------------------------------

/**
 * Adds `.compliance()` to a builder AND remaps chain methods so
 * `.compliance()` persists. Each chain method returns WithCompliance
 * wrapping the chain result. `.compliance()` captures the current
 * builder as-is into ClassifiedProperty.
 */
export type WithCompliance<T> = {
  [K in keyof T]: T[K] extends (...args: infer A) => infer R
    ? R extends { '~type'?: unknown; '~options': unknown }
      ? (...args: A) => WithCompliance<R>
      : T[K]
    : T[K];
} & {
  compliance(
    level: ComplianceLevel
  ): T extends { '~type'?: { value: infer V }; '~options': infer O }
    ? ClassifiedProperty<T, V, O>
    : ClassifiedProperty<T>;
};

// ---------------------------------------------------------------------------
// ForklaunchPropertyBuilders — the type of `fp`
// ---------------------------------------------------------------------------

type RelationBuilderKeys =
  | 'manyToOne'
  | 'oneToMany'
  | 'manyToMany'
  | 'oneToOne'
  | 'embedded';

// Generic methods whose type params get collapsed by mapped types
type GenericBuilderKeys =
  | 'json'
  | 'formula'
  | 'type'
  | 'enum'
  | 'bigint'
  | 'array'
  | 'decimal';

/**
 * Each scalar method wraps the EXACT MikroORM return type with
 * WithCompliance. Relations pass through unchanged.
 * Generic methods have explicit signatures to preserve type params.
 */
export type ForklaunchPropertyBuilders = {
  [K in Exclude<
    keyof PropertyBuilders,
    RelationBuilderKeys | GenericBuilderKeys
  >]: PropertyBuilders[K] extends (...args: infer A) => infer R
    ? (...args: A) => WithCompliance<R>
    : PropertyBuilders[K];
} & {
  [K in RelationBuilderKeys]: PropertyBuilders[K];
} & {
  json: <T>() => WithCompliance<PropertyChain<T, EmptyOptions>>;
  formula: <T>(
    formula: string | ((...args: never[]) => string)
  ) => WithCompliance<PropertyChain<T, EmptyOptions>>;
  type: <T>(type: T) => WithCompliance<PropertyChain<T, EmptyOptions>>;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  enum: <const T extends (number | string)[] | (() => Record<string, any>)>(
    items?: T
  ) => WithCompliance<
    PropertyChain<
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      T extends () => Record<string, any>
        ? T extends () => infer R
          ? R[keyof R]
          : never
        : T extends (infer Value)[]
          ? Value
          : T,
      EmptyOptions & { kind: 'enum' }
    >
  >;
  bigint: <Mode extends 'bigint' | 'number' | 'string' = 'bigint'>(
    mode?: Mode
  ) => WithCompliance<
    PropertyChain<
      Mode extends 'bigint' ? bigint : Mode extends 'number' ? number : string,
      EmptyOptions
    >
  >;
  array: <T = string>(
    toJsValue?: (i: string) => T,
    toDbValue?: (i: T) => string
  ) => WithCompliance<PropertyChain<T[], EmptyOptions>>;
  decimal: <Mode extends 'number' | 'string' = 'string'>(
    mode?: Mode
  ) => WithCompliance<
    PropertyChain<Mode extends 'number' ? number : string, EmptyOptions>
  >;
};
