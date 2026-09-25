import { wrapEmWithTenantContext } from '@forklaunch/core/persistence';
import { EntityManager } from '@mikro-orm/core';

// Encrypted columns (saved searches, history) are keyed per organization, so
// they are written and read through an entity manager bound to it.
export function tenantEm(em: EntityManager, organizationId: string): EntityManager {
  return wrapEmWithTenantContext(em.fork(), organizationId) as EntityManager;
}
