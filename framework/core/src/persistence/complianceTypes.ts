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
// Retention types and registry
// ---------------------------------------------------------------------------

export const RetentionAction = {
  delete: 'delete',
  anonymize: 'anonymize'
} as const;
export type RetentionAction =
  (typeof RetentionAction)[keyof typeof RetentionAction];

export interface RetentionPolicy {
  duration: string;
  action: RetentionAction;
}

export const RetentionDuration = {
  days: (n: number): string => `P${n}D`,
  months: (n: number): string => `P${n}M`,
  years: (n: number): string => `P${n}Y`
} as const;

export interface ParsedDuration {
  years: number;
  months: number;
  days: number;
}

const DURATION_REGEX = /^P(?:(\d+)Y)?(?:(\d+)M)?(?:(\d+)D)?$/;

/**
 * Parse an ISO 8601 duration string into calendar units.
 * Returns structured units to enable calendar-aware date arithmetic.
 */
export function parseDuration(iso: string): ParsedDuration {
  const match = DURATION_REGEX.exec(iso);
  if (!match) {
    throw new Error(
      `Invalid ISO 8601 duration: '${iso}'. Expected format: P[n]Y[n]M[n]D`
    );
  }
  const years = parseInt(match[1] || '0', 10);
  const months = parseInt(match[2] || '0', 10);
  const days = parseInt(match[3] || '0', 10);

  // Approximate total days for minimum validation only
  const approxDays = years * 365 + months * 30 + days;
  if (approxDays < 1) {
    throw new Error(
      `Retention duration must be >= 1 day (P1D). Got: '${iso}' (${approxDays} approx days)`
    );
  }

  return { years, months, days };
}

/**
 * Subtract a parsed duration from a date using calendar-aware arithmetic.
 * Handles month-end clamping and leap years correctly.
 */
export function subtractDuration(from: Date, duration: ParsedDuration): Date {
  const result = new Date(from);
  result.setFullYear(result.getFullYear() - duration.years);
  result.setMonth(result.getMonth() - duration.months);
  result.setDate(result.getDate() - duration.days);
  return result;
}

const retentionRegistry = new Map<string, RetentionPolicy>();

// ---------------------------------------------------------------------------
// User ID field registry — maps entity name to the field linking records to a user
// ---------------------------------------------------------------------------

const DEFAULT_USER_ID_FIELD = 'userId';
const userIdFieldRegistry = new Map<string, string>();

export function registerEntityUserIdField(
  entityName: string,
  field: string
): void {
  userIdFieldRegistry.set(entityName, field);
}

export function getEntityUserIdField(entityName: string): string {
  return userIdFieldRegistry.get(entityName) ?? DEFAULT_USER_ID_FIELD;
}

export function getAllUserIdFields(): ReadonlyMap<string, string> {
  return userIdFieldRegistry;
}

export function registerEntityRetention(
  entityName: string,
  policy: RetentionPolicy
): void {
  retentionRegistry.set(entityName, policy);
}

export function getEntityRetention(
  entityName: string
): RetentionPolicy | undefined {
  return retentionRegistry.get(entityName);
}

export function getAllRetentionPolicies(): ReadonlyMap<
  string,
  RetentionPolicy
> {
  return retentionRegistry;
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
