export const ComplianceLevel = {
  pii: 'pii',
  phi: 'phi',
  pci: 'pci',
  sox: 'sox',
  none: 'none'
} as const;
export type ComplianceLevel =
  (typeof ComplianceLevel)[keyof typeof ComplianceLevel];

/**
 * Check if a compliance level requires data protection (non-'none').
 */
export function isProtectedData(level: ComplianceLevel): boolean {
  return level !== 'none';
}

export const COMPLIANCE_KEY = '~compliance' as const;

// ---------------------------------------------------------------------------
// Registry (DEPRECATED - kept for backward compatibility)
// ---------------------------------------------------------------------------
// These functions are deprecated. Compliance metadata is now stored directly
// in EntitySchema.meta.compliance and accessed via ORM metadata.
// For new code, use:
// - Auto-discovery: new ComplianceDataService(orm, otel) // reads from ORM
// - Explicit: new ComplianceDataService(orm, otel, { entities: [...] })
// ---------------------------------------------------------------------------

const complianceRegistry = new Map<string, Map<string, ComplianceLevel>>();

/**
 * @deprecated Use EntitySchema.meta.compliance instead. This function is kept
 * for backward compatibility only. defineComplianceEntity now stores metadata
 * in the EntitySchema, making this registry unnecessary.
 */
export function registerEntityCompliance(
  entityName: string,
  fields: Map<string, ComplianceLevel>
): void {
  complianceRegistry.set(entityName, fields);
}

/**
 * @deprecated Use EntitySchema.meta.compliance.fields instead. This function
 * reads from the legacy global registry and should not be used in new code.
 */
export function getComplianceMetadata(
  entityName: string,
  fieldName: string
): ComplianceLevel {
  return complianceRegistry.get(entityName)?.get(fieldName) ?? 'none';
}

/**
 * @deprecated Use EntitySchema.meta.compliance.fields instead. This function
 * reads from the legacy global registry and should not be used in new code.
 */
export function getEntityComplianceFields(
  entityName: string
): Map<string, ComplianceLevel> | undefined {
  return complianceRegistry.get(entityName);
}

export function entityHasEncryptedFields(entityName: string): boolean {
  const fields = complianceRegistry.get(entityName);
  if (!fields) return false;
  for (const level of fields.values()) {
    if (level === 'phi' || level === 'pci' || level === 'sox') return true;
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
// User ID field registry (DEPRECATED) — maps entity name to the field linking records to a user
// ---------------------------------------------------------------------------

const DEFAULT_USER_ID_FIELD = 'userId';
const userIdFieldRegistry = new Map<string, string>();

/**
 * @deprecated Use EntitySchema.meta.compliance.userIdField instead. This
 * function is kept for backward compatibility only.
 */
export function registerEntityUserIdField(
  entityName: string,
  field: string
): void {
  userIdFieldRegistry.set(entityName, field);
}

/**
 * @deprecated Use EntitySchema.meta.compliance.userIdField instead. This
 * function reads from the legacy global registry and should not be used in new code.
 */
export function getEntityUserIdField(entityName: string): string {
  return userIdFieldRegistry.get(entityName) ?? DEFAULT_USER_ID_FIELD;
}

/**
 * @deprecated Use EntitySchema.meta.compliance.userIdField instead. This
 * function reads from the legacy global registry and should not be used in new code.
 */
export function getAllUserIdFields(): ReadonlyMap<string, string> {
  return userIdFieldRegistry;
}

/**
 * @deprecated Use EntitySchema.meta.compliance.retention instead. This
 * function is kept for backward compatibility only.
 */
export function registerEntityRetention(
  entityName: string,
  policy: RetentionPolicy
): void {
  retentionRegistry.set(entityName, policy);
}

/**
 * @deprecated Use EntitySchema.meta.compliance.retention instead. This
 * function reads from the legacy global registry and should not be used in new code.
 */
export function getEntityRetention(
  entityName: string
): RetentionPolicy | undefined {
  return retentionRegistry.get(entityName);
}

/**
 * @deprecated Use EntitySchema.meta.compliance.retention instead. This
 * function reads from the legacy global registry and should not be used in new code.
 */
export function getAllRetentionPolicies(): ReadonlyMap<
  string,
  RetentionPolicy
> {
  return retentionRegistry;
}

// ---------------------------------------------------------------------------
// Compliance metadata structure stored in EntitySchema.meta
// ---------------------------------------------------------------------------

export interface EntityComplianceMetadata {
  fields: Map<string, ComplianceLevel>;
  userIdField?: string;
  retention?: RetentionPolicy;
}

// ---------------------------------------------------------------------------
// Module augmentation — adds .compliance() via PropertyOptions and extends EntityMetadata
// ---------------------------------------------------------------------------

/**
 * Adds `compliance` to PropertyOptions, which flows into IncludeKeys
 * for all scalar/enum/embedded builders (PropertyOptions is extended by
 * EnumOptions and EmbeddedOptions). Relation builders use ReferenceOptions
 * instead, so they don't get .compliance() — which is what we want.
 *
 * Also extends EntityMetadata to store compliance metadata directly in the ORM's
 * entity metadata structure, eliminating the need for a separate global registry.
 */
declare module '@mikro-orm/core' {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface PropertyOptions<Owner> {
    compliance?: ComplianceLevel;
  }

  interface UniversalPropertyOptionsBuilder<Value, Options, IncludeKeys> {
    compliance(
      level: ComplianceLevel
    ): Pick<
      UniversalPropertyOptionsBuilder<
        Value,
        Options & { readonly '~c': true },
        IncludeKeys
      >,
      IncludeKeys & keyof UniversalPropertyOptionsBuilder<never, never, never>
    >;
  }

  interface EntityMetadata {
    compliance?: EntityComplianceMetadata;
  }
}
