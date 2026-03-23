import type {
  Dictionary,
  EntityManager,
  FilterDef,
  MikroORM
} from '@mikro-orm/core';

/**
 * The name used to register the tenant isolation filter.
 */
export const TENANT_FILTER_NAME = 'tenant';

/**
 * Creates the tenant filter definition.
 *
 * The filter adds `WHERE organizationId = :tenantId` to all queries
 * on entities that have an `organizationId` or `organization` property.
 * Entities without either property are unaffected (empty condition).
 */
export function createTenantFilterDef(): FilterDef {
  return {
    name: TENANT_FILTER_NAME,
    cond(
      args: Dictionary,
      _type: 'read' | 'update' | 'delete',
      em: EntityManager,
      _options?: unknown,
      entityName?: string
    ) {
      if (!entityName) {
        return {};
      }

      try {
        const metadata = em.getMetadata().getByClassName(entityName, false);
        if (!metadata) {
          return {};
        }

        const hasOrganizationId = metadata.properties['organizationId'] != null;
        const hasOrganization = metadata.properties['organization'] != null;

        if (hasOrganizationId || hasOrganization) {
          return { organizationId: args.tenantId };
        }
      } catch {
        // Entity not found in metadata — skip filtering
      }

      return {};
    },
    default: true,
    args: true
  };
}

/**
 * Registers the global tenant isolation filter on the ORM's entity manager.
 * Call this once at application bootstrap after `MikroORM.init()`.
 *
 * After calling this, every fork of the EM will inherit the filter.
 * Set the tenant ID per-request via:
 *
 * ```ts
 * em.setFilterParams('tenant', { tenantId: 'org-123' });
 * ```
 */
export function setupTenantFilter(orm: MikroORM): void {
  orm.em.addFilter(createTenantFilterDef());
}

/**
 * Returns a forked EntityManager with the tenant filter disabled.
 *
 * Use this only from code paths that have verified super-admin permissions.
 * Queries executed through the returned EM will return cross-tenant data.
 */
export function getSuperAdminContext(em: EntityManager): EntityManager {
  const forked = em.fork();
  forked.setFilterParams(TENANT_FILTER_NAME, { tenantId: undefined });
  // Disable the filter by passing false for the filter in each query isn't
  // sufficient globally; instead we add the filter with enabled = false.
  // The cleanest way is to re-add the filter as disabled on this fork.
  forked.addFilter({
    name: TENANT_FILTER_NAME,
    cond: {},
    default: false
  });
  return forked;
}
