/**
 * A COMPLIANCE WALK CANNOT BE TENANT-BLIND.
 *
 * `erase` and `export` search every PII-bearing entity for a subject's rows.
 * Every one of those columns is encrypted under its row's tenant, and a WHERE
 * value on an encrypted column is encrypted under the CURRENT tenant before it
 * is compared — so an unbound walk searches with a key nothing was written
 * with. It matches nothing, everywhere.
 *
 * That is the worst possible failure for this particular service: an erase
 * that finds nothing is indistinguishable from an erase with nothing to do,
 * and it reported success. A subject was told their data was deleted while all
 * of it remained.
 *
 * Two changes, tested here. The walk runs once per tenant in scope, and an
 * entity it could not read is REPORTED rather than logged and forgotten.
 */
import { describe, expect, it, vi } from 'vitest';
import {
  ComplianceDataService,
  type ComplianceOrm
} from '../src/services/complianceDataService';
import { fp } from '../src/persistence/compliancePropertyBuilder';
import { defineComplianceEntity } from '../src/persistence/defineComplianceEntity';
import { getCurrentTenantId } from '../src/persistence/encryptedType';

defineComplianceEntity({
  name: 'TenantScopedNote',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    userId: fp.string().compliance('none'),
    body: fp.string().compliance('pii')
  }
});

const ORG_A = 'org-a';
const ORG_B = 'org-b';
const PLATFORM = '_internal';

const otel = {
  info: vi.fn(),
  warn: vi.fn(),
  error: vi.fn(),
  debug: vi.fn()
} as never;

/**
 * An ORM whose `find` answers only when the right tenant is bound — which is
 * how the real thing behaves once a column is encrypted.
 */
function ormReturningRowsFor(
  rowsByTenant: Record<string, { id: string; userId: string }[]>,
  options: { throwFor?: string } = {}
): { orm: ComplianceOrm; seen: string[] } {
  const seen: string[] = [];
  const em = {
    find: async () => {
      const tenant = getCurrentTenantId();
      seen.push(tenant);
      if (options.throwFor !== undefined && tenant === options.throwFor) {
        throw new Error(`Cannot read an encrypted column under ${tenant}`);
      }
      return rowsByTenant[tenant] ?? [];
    },
    remove: () => undefined,
    flush: async () => undefined,
    count: async () => 0
  };
  const metadata = {
    getAll: () =>
      new Map([
        [
          'TenantScopedNote',
          {
            className: 'TenantScopedNote',
            class: 'TenantScopedNote',
            properties: { id: {}, userId: {}, body: {} }
          }
        ]
      ])
  };
  return {
    seen,
    orm: {
      em: { fork: () => em },
      getMetadata: () => metadata
    } as unknown as ComplianceOrm
  };
}

describe('compliance walks are tenant-scoped', () => {
  it('searches once per tenant in scope', async () => {
    const { orm, seen } = ormReturningRowsFor({});
    const service = new ComplianceDataService(orm, otel, {
      TenantScopedNote: 'userId'
    });
    await service.erase('user-1', { tenantIds: [ORG_A, ORG_B, PLATFORM] });
    expect(seen).toEqual([ORG_A, ORG_B, PLATFORM]);
  });

  it('finds rows the unbound walk could never have matched', async () => {
    const { orm } = ormReturningRowsFor({
      [ORG_B]: [{ id: 'n1', userId: 'user-1' }]
    });
    const service = new ComplianceDataService(orm, otel, {
      TenantScopedNote: 'userId'
    });

    // Unbound: the old behaviour. Nothing found, "success" reported.
    const blind = await service.erase('user-1');
    expect(blind.recordsDeleted).toBe(0);

    // Scoped: the row is where it always was.
    const scoped = await service.erase('user-1', {
      tenantIds: [ORG_A, ORG_B]
    });
    expect(scoped.recordsDeleted).toBe(1);
    expect(scoped.entitiesAffected).toEqual(['TenantScopedNote']);
  });

  it('reports an entity it could not read instead of swallowing it', async () => {
    const { orm } = ormReturningRowsFor(
      { [ORG_A]: [{ id: 'n1', userId: 'user-1' }] },
      { throwFor: ORG_B }
    );
    const service = new ComplianceDataService(orm, otel, {
      TenantScopedNote: 'userId'
    });
    const result = await service.erase('user-1', {
      tenantIds: [ORG_A, ORG_B]
    });

    // The rows it COULD erase are still erased ...
    expect(result.recordsDeleted).toBe(1);
    // ... and the caller is told the job is incomplete.
    expect(result.failures).toHaveLength(1);
    expect(result.failures[0].entityName).toBe('TenantScopedNote');
    expect(result.failures[0].tenantId).toBe(ORG_B);
    expect(result.failures[0].reason).toMatch(/encrypted column/);
  });

  it('merges a subject-access export across tenants rather than overwriting', async () => {
    const { orm } = ormReturningRowsFor({
      [ORG_A]: [{ id: 'a1', userId: 'user-1' }],
      [ORG_B]: [{ id: 'b1', userId: 'user-1' }]
    });
    const service = new ComplianceDataService(orm, otel, {
      TenantScopedNote: 'userId'
    });
    const result = await service.export('user-1', {
      tenantIds: [ORG_A, ORG_B]
    });
    // The second pass must not erase the first pass's findings.
    expect(result.entities['TenantScopedNote']).toHaveLength(2);
    expect(result.failures).toEqual([]);
  });

  it('still runs once, unbound, when no tenant is named', async () => {
    // Correct for a schema whose retained entities carry no encrypted column,
    // and loud rather than silent for one that does.
    const { orm, seen } = ormReturningRowsFor({});
    const service = new ComplianceDataService(orm, otel, {
      TenantScopedNote: 'userId'
    });
    await service.export('user-1');
    expect(seen).toEqual(['']);
  });
});
