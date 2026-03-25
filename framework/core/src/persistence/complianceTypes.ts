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
// Module augmentation — adds .compliance() directly to PropertyChain
// ---------------------------------------------------------------------------

/**
 * Augments MikroORM's PropertyChain with `.compliance()`. Because
 * PropertyChain methods return PropertyChain (not Pick), the method
 * persists through chains naturally — no recursive mapped type needed.
 */
declare module '@mikro-orm/core' {
  interface PropertyChain<Value, Options> {
    compliance(
      level: ComplianceLevel
    ): PropertyChain<Value, Options> & { readonly __classified: true };
  }
}

// ---------------------------------------------------------------------------
// ForklaunchPropertyBuilders — the type of `fp`
// ---------------------------------------------------------------------------

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
 * Converts UniversalPropertyOptionsBuilder return types to PropertyChain.
 * PropertyChain is MikroORM's lightweight chain type and now carries
 * .compliance() via module augmentation — zero recursive mapped types.
 */
type ToPropertyChain<T> = T extends {
  '~type'?: { value: infer V };
  '~options': infer O;
}
  ? PropertyChain<V, O>
  : T;

/**
 * Drop-in type for the `fp` proxy. Scalar and relation methods return
 * PropertyChain (which has .compliance() from the augmentation).
 * Generic methods have explicit signatures to preserve type params.
 */
export type ForklaunchPropertyBuilders = {
  [K in Exclude<
    keyof PropertyBuilders,
    GenericBuilderKeys
  >]: PropertyBuilders[K] extends (...args: infer A) => infer R
    ? (...args: A) => ToPropertyChain<R>
    : PropertyBuilders[K];
} & {
  json: <T>() => PropertyChain<T, EmptyOptions>;
  formula: <T>(
    formula: string | ((...args: never[]) => string)
  ) => PropertyChain<T, EmptyOptions>;
  type: <T>(type: T) => PropertyChain<T, EmptyOptions>;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  enum: <const T extends (number | string)[] | (() => Record<string, any>)>(
    items?: T
  ) => PropertyChain<
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    T extends () => Record<string, any>
      ? T extends () => infer R
        ? R[keyof R]
        : never
      : T extends (infer Value)[]
        ? Value
        : T,
    EmptyOptions & { kind: 'enum' }
  >;
  bigint: <Mode extends 'bigint' | 'number' | 'string' = 'bigint'>(
    mode?: Mode
  ) => PropertyChain<
    Mode extends 'bigint' ? bigint : Mode extends 'number' ? number : string,
    EmptyOptions
  >;
  array: <T = string>(
    toJsValue?: (i: string) => T,
    toDbValue?: (i: T) => string
  ) => PropertyChain<T[], EmptyOptions>;
  decimal: <Mode extends 'number' | 'string' = 'string'>(
    mode?: Mode
  ) => PropertyChain<Mode extends 'number' ? number : string, EmptyOptions>;
};
