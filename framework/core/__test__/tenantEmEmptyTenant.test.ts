import { describe, expect, it } from 'vitest';
import {
  FieldEncryptor,
  getCurrentTenantId,
  registerEncryptor,
  withEncryptionContext,
  wrapEmWithTenantContext
} from '../src/persistence';

/**
 * `wrapEmWithTenantContext(em, '')` must bind the EMPTY tenant, and creating
 * a wrapped EM must never change the tenant the caller is running under.
 * Both held false before: '' was treated as "do not wrap", and the wrapper
 * called `setEncryptionTenantId`, whose `enterWith` leaked the tenant into
 * the caller's async resource for everything that ran afterwards.
 */

const org = '302fbb63-1710-4738-aa60-d1fcabd2988f';

/** A stand-in EntityManager: every method reports the tenant it ran under. */
function fakeEm(calls: string[] = []) {
  return {
    calls,
    setFilterParams: (_name: string, params: { tenantId: string }) =>
      calls.push(`filter:${params.tenantId}`),
    findOne: async () => {
      calls.push(`findOne:${JSON.stringify(getCurrentTenantId())}`);
      return null;
    },
    flush: async () => {
      calls.push(`flush:${JSON.stringify(getCurrentTenantId())}`);
    },
    persist(this: unknown) {
      calls.push(`persist:${JSON.stringify(getCurrentTenantId())}`);
      return this;
    },
    fork() {
      return fakeEm(calls);
    }
  };
}

registerEncryptor(new FieldEncryptor('tenant-em-test-master-key'));

describe('wrapEmWithTenantContext and the empty tenant', () => {
  it("binds '' as a real tenant for every EM call", async () => {
    const em = fakeEm();
    const wrapped = wrapEmWithTenantContext(em as never, '');
    expect(wrapped).not.toBe(em);
    await withEncryptionContext(org, async () => {
      await wrapped.findOne('X' as never, {});
      await wrapped.flush();
    });
    expect(em.calls).toEqual(['findOne:""', 'flush:""']);
  });

  it('does not set the tenant filter for the empty tenant, and does for a real one', () => {
    const empty = fakeEm();
    wrapEmWithTenantContext(empty as never, '');
    expect(empty.calls).toEqual([]);
    const real = fakeEm();
    wrapEmWithTenantContext(real as never, org);
    expect(real.calls).toEqual([`filter:${org}`]);
  });

  it('leaves the EM unwrapped only for undefined', () => {
    const em = fakeEm();
    expect(wrapEmWithTenantContext(em as never, undefined)).toBe(em);
  });

  it('creating a wrapped EM never leaks its tenant into the caller', async () => {
    expect(getCurrentTenantId()).toBe('');
    const em = fakeEm();
    wrapEmWithTenantContext(em as never, org);
    expect(getCurrentTenantId()).toBe('');
    // What the promote does: an org EM exists, then a no-tenant EM is used.
    const global = fakeEm();
    const wrappedGlobal = wrapEmWithTenantContext(global as never, '');
    await wrappedGlobal.findOne('X' as never, {});
    expect(global.calls).toEqual(['findOne:""']);
  });

  it('an org EM binds the org for its calls and nothing else', async () => {
    const em = fakeEm();
    const wrapped = wrapEmWithTenantContext(em as never, org);
    await wrapped.findOne('X' as never, {});
    expect(em.calls).toEqual([
      `filter:${org}`,
      `findOne:${JSON.stringify(org)}`
    ]);
    expect(getCurrentTenantId()).toBe('');
  });

  it('keeps a chained persist().flush() inside the tenant', async () => {
    // `persist` returns the entity manager. Handed back raw, the chained
    // `flush` would run outside the context and encrypt the row under the
    // ambient tenant; that is how an organization's subscription ended up
    // written under the empty key by an operator promote.
    const em = fakeEm();
    const wrapped = wrapEmWithTenantContext(em as never, org);
    const chained = wrapped.persist({} as never);
    expect(chained).toBe(wrapped);
    await chained.flush();
    expect(em.calls).toEqual([
      `filter:${org}`,
      `persist:"${org}"`,
      `flush:"${org}"`
    ]);
  });

  it('leaves a fork unbound so an explicit context around it wins', async () => {
    // getSuperAdminContext(em).fork() inside withEncryptionContext(other, …)
    // is how a service steps out of the tenant on purpose.
    const em = fakeEm();
    const wrapped = wrapEmWithTenantContext(em as never, org);
    const forked = wrapped.fork();
    await withEncryptionContext('', async () => {
      await forked.flush();
    });
    expect(em.calls).toEqual([`filter:${org}`, 'flush:""']);
  });
});
