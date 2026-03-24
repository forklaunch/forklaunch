// Compliance types and registry
export {
  ComplianceLevel,
  type ComplianceLevel as ComplianceLevelType,
  type ClassifiedProperty,
  type ExtractInner,
  type ForklaunchPropertyBuilders,
  type WithCompliance,
  getComplianceMetadata,
  getEntityComplianceFields,
  entityHasEncryptedFields
} from './complianceTypes';

// Compliance-aware property builder (drop-in replacement for MikroORM's p)
export { fp } from './compliancePropertyBuilder';

// Compliance-aware entity definition (drop-in replacement for MikroORM's defineEntity)
export { defineComplianceEntity } from './defineComplianceEntity';

// Compliance EventSubscriber (encrypt on persist, decrypt on load)
export {
  ComplianceEventSubscriber,
  wrapEmWithNativeQueryBlocking
} from './complianceEventSubscriber';

// Re-export InferEntity from MikroORM for convenience
export type { InferEntity } from '@mikro-orm/core';

// Tenant isolation filter
export {
  setupTenantFilter,
  getSuperAdminContext,
  createTenantFilterDef,
  TENANT_FILTER_NAME
} from './tenantFilter';

// PostgreSQL Row-Level Security
export { setupRls, RlsEventSubscriber, type RlsConfig } from './rls';
