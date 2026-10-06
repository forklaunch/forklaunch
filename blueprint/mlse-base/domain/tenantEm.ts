import { wrapEmWithTenantContext } from '@forklaunch/core/persistence';
import { EntityManager } from '@mikro-orm/core';

// Encrypted columns (saved searches, history, answer queries) are keyed per
// organization, so they are written and read through an entity manager bound
// to it. An answer requested without an organization is encrypted under
// NO_ORGANIZATION_TENANT.
export const NO_ORGANIZATION_TENANT = 'mlse:no-organization';

export function tenantEm(em: EntityManager, organizationId: string): EntityManager {
  return wrapEmWithTenantContext(em.fork(), organizationId) as EntityManager;
}
