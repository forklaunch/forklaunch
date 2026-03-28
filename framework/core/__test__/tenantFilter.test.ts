import { describe, expect, it, vi } from 'vitest';
import {
  setupTenantFilter,
  getSuperAdminContext,
  createTenantFilterDef,
  TENANT_FILTER_NAME
} from '../src/persistence/tenantFilter';

describe('createTenantFilterDef', () => {
  it('returns a filter definition with the correct name', () => {
    const def = createTenantFilterDef();
    expect(def.name).toBe('tenant');
  });

  it('has default enabled', () => {
    const def = createTenantFilterDef();
    expect(def.default).toBe(true);
  });

  it('has args disabled to allow queries without tenant context', () => {
    const def = createTenantFilterDef();
    expect(def.args).toBe(false);
  });

  describe('cond function', () => {
    function makeMockEm(properties: Record<string, unknown>): unknown {
      return {
        getMetadata: () => ({
          getByClassName: () => ({ properties })
        })
      };
    }

    it('returns organizationId condition for entities with organizationId property', () => {
      const def = createTenantFilterDef();
      const cond = def.cond as (...args: unknown[]) => unknown;
      const em = makeMockEm({ organizationId: { name: 'organizationId' } });

      const result = cond(
        { tenantId: 'org-123' },
        'read',
        em,
        undefined,
        'TenantEntity'
      );

      expect(result).toEqual({ organizationId: 'org-123' });
    });

    it('returns organization relation condition for entities with organization relation (no scalar)', () => {
      const def = createTenantFilterDef();
      const cond = def.cond as (...args: unknown[]) => unknown;
      const em = makeMockEm({ organization: { name: 'organization' } });

      const result = cond(
        { tenantId: 'org-456' },
        'read',
        em,
        undefined,
        'RelatedEntity'
      );

      expect(result).toEqual({ organization: 'org-456' });
    });

    it('returns empty condition for entities without organizationId or organization', () => {
      const def = createTenantFilterDef();
      const cond = def.cond as (...args: unknown[]) => unknown;
      const em = makeMockEm({
        name: { name: 'name' },
        email: { name: 'email' }
      });

      const result = cond(
        { tenantId: 'org-789' },
        'read',
        em,
        undefined,
        'UnrelatedEntity'
      );

      expect(result).toEqual({});
    });

    it('returns empty condition when entityName is undefined', () => {
      const def = createTenantFilterDef();
      const cond = def.cond as (...args: unknown[]) => unknown;
      const em = makeMockEm({});

      const result = cond(
        { tenantId: 'org-123' },
        'read',
        em,
        undefined,
        undefined
      );

      expect(result).toEqual({});
    });

    it('returns empty condition when metadata lookup throws', () => {
      const def = createTenantFilterDef();
      const cond = def.cond as (...args: unknown[]) => unknown;
      const em: unknown = {
        getMetadata: () => ({
          getByClassName: () => {
            throw new Error('Entity not found');
          }
        })
      };

      const result = cond(
        { tenantId: 'org-123' },
        'read',
        em,
        undefined,
        'MissingEntity'
      );

      expect(result).toEqual({});
    });

    it('works for update and delete operation types', () => {
      const def = createTenantFilterDef();
      const cond = def.cond as (...args: unknown[]) => unknown;
      const em = makeMockEm({ organizationId: { name: 'organizationId' } });

      for (const type of ['update', 'delete'] as const) {
        const result = cond(
          { tenantId: 'org-abc' },
          type,
          em,
          undefined,
          'TenantEntity'
        );
        expect(result).toEqual({ organizationId: 'org-abc' });
      }
    });
  });
});

describe('setupTenantFilter', () => {
  it('registers the filter on the ORM entity manager', () => {
    const addFilter = vi.fn();
    const mockOrm = {
      em: { addFilter }
    } as Parameters<typeof setupTenantFilter>[0];

    setupTenantFilter(mockOrm);

    expect(addFilter).toHaveBeenCalledOnce();
    const filterDef = addFilter.mock.calls[0][0];
    expect(filterDef.name).toBe(TENANT_FILTER_NAME);
    expect(filterDef.default).toBe(true);
    expect(typeof filterDef.cond).toBe('function');
  });
});

describe('getSuperAdminContext', () => {
  it('returns a forked EntityManager', () => {
    const forkedEm = {
      setFilterParams: vi.fn(),
      addFilter: vi.fn()
    };
    const em = {
      fork: vi.fn().mockReturnValue(forkedEm)
    } as Parameters<typeof getSuperAdminContext>[0];

    const result = getSuperAdminContext(em);

    expect(em.fork).toHaveBeenCalledOnce();
    expect(result).toBe(forkedEm);
  });

  it('disables the tenant filter on the forked EM', () => {
    const forkedEm = {
      setFilterParams: vi.fn(),
      addFilter: vi.fn()
    };
    const em = {
      fork: vi.fn().mockReturnValue(forkedEm)
    } as Parameters<typeof getSuperAdminContext>[0];

    getSuperAdminContext(em);

    // Verifies the filter is re-added with default: false
    expect(forkedEm.addFilter).toHaveBeenCalledWith(
      expect.objectContaining({
        name: TENANT_FILTER_NAME,
        default: false
      })
    );
  });

  it('clears filter params on the forked EM', () => {
    const forkedEm = {
      setFilterParams: vi.fn(),
      addFilter: vi.fn()
    };
    const em = {
      fork: vi.fn().mockReturnValue(forkedEm)
    } as Parameters<typeof getSuperAdminContext>[0];

    getSuperAdminContext(em);

    expect(forkedEm.setFilterParams).toHaveBeenCalledWith(TENANT_FILTER_NAME, {
      tenantId: undefined
    });
  });
});
