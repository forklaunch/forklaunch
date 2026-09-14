// Compliance types and registry
export {
  ComplianceLevel,
  type ComplianceLevel as ComplianceLevelType,
  getComplianceMetadata,
  getEntityComplianceFields,
  entityHasEncryptedFields,
  // Retention
  RetentionAction,
  type RetentionAction as RetentionActionType,
  type RetentionPolicy,
  RetentionDuration,
  parseDuration,
  subtractDuration,
  type ParsedDuration,
  getEntityRetention,
  getAllRetentionPolicies,
  getEntityUserIdField,
  getAllUserIdFields,
  type EncryptedComplianceLevel,
  type EncryptedKeysOf,
  type RequiresEncryptionContext,
  type SelectionAvoidsEncryptedColumns
} from './complianceTypes';
export {
  wrapEmWithForgivingDecryption,
  type ForgivingDecryptionLogger
} from './forgivingDecryptionEm';
export {
  asEncryptionSafe,
  type ContextFreeKeysOfSchema,
  type ContextFreeReadOptions,
  type DecryptingReadOptions,
  type EncryptedKeysOfSchema,
  type EncryptionAwareReadOptions,
  type EncryptionSafeEntityManager,
  type HydratingReadMethod,
  type PropertiesOfSchema,
  type SchemaRequiresEncryptionContext
} from './encryptionSafeEm';

// Compliance-aware property builder (drop-in replacement for MikroORM's p)
export { fp } from './compliancePropertyBuilder';

// Compliance-aware entity definition (drop-in replacement for MikroORM's defineEntity)
export { defineComplianceEntity } from './defineComplianceEntity';

// Native query blocking (prevents bypassing EncryptedType via raw queries)
export { wrapEmWithNativeQueryBlocking } from './complianceEventSubscriber';

// Encrypted custom type (transparent field encryption at the MikroORM data layer)
export {
  EncryptedType,
  registerEncryptor,
  setEncryptionTenantId,
  withEncryptionContext,
  getCurrentTenantId
} from './encryptedType';

// Field encryption
export {
  FieldEncryptor,
  MissingEncryptionKeyError,
  DecryptionError,
  EncryptionRequiredError
} from './fieldEncryptor';

// Tenant isolation filter
export {
  setupTenantFilter,
  getSuperAdminContext,
  createTenantFilterDef,
  TENANT_FILTER_NAME
} from './tenantFilter';

// PostgreSQL Row-Level Security
export { setupRls, RlsEventSubscriber, type RlsConfig } from './rls';

export type { AnyMikroORM, ResolvedEntity } from './mikroOrm.types';

// Tenant-scoped EM proxy (wraps every operation in withEncryptionContext to
// survive AsyncLocalStorage propagation through pg connection pool callbacks)
export { wrapEmWithTenantContext } from './tenantEm';
