import { describe, expect, it } from 'vitest';
import {
  EmptyTenantError,
  FieldEncryptor,
  getBoundTenantId,
  getCurrentTenantId,
  registerEncryptor,
  withEncryptionContext,
  wrapEmWithTenantContext
} from '../src/persistence';

/**
 * `''` IS NOT A TENANT.
 *
 * It used to be: `wrapEmWithTenantContext(em, '')` bound "the no-tenant key",
 * and global rows — a billing plan, a trial, a template — were encrypted under
 * it. That made a deliberate global row and a tenant nobody resolved the same
 * key, so a path that simply forgot to bind wrote rows that looked perfectly
 * fine. The mistake surfaced later and elsewhere, as the owning tenant's
 * "Failed to decrypt encrypted column value", with a stack trace pointing at
 * the innocent reader.
 *
 * Every door that binds a tenant now rejects `''`, and touching an encrypted
 * column with nothing bound raises `UnboundTenantError` instead of quietly
 * using the empty key. Global rows take an explicit constant — the platform
 * uses `'_internal'` — which is a value you can name, search for, and migrate.
 *
 * Creating a wrapped EM must also never change the tenant the caller runs
 * under: the wrapper used to call `setEncryptionTenantId`, whose `enterWith`
 * leaked its tenant into the caller's async resource for everything that ran
 * afterwards.
 */

const org = '302fbb63-1710-4738-aa60-d1fcabd2988f';
const platform = '_internal';

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

describe('the empty tenant is refused', () => {
  it('refuses to bind an EM to the empty tenant', () => {
    const em = fakeEm();
    expect(() => wrapEmWithTenantContext(em as never, '')).toThrow(
      EmptyTenantError
    );
    // Nothing is half-done: the filter is not set on the way out.
    expect(em.calls).toEqual([]);
  });

  it('refuses to open an encryption context on the empty tenant', () => {
    expect(() => withEncryptionContext('', () => 'unreachable')).toThrow(
      EmptyTenantError
    );
  });

  it('says what to do instead', () => {
    expect(() => withEncryptionContext('', () => null)).toThrow(
      /not a tenant id[\s\S]*_internal/
    );
  });

  it('takes an explicit platform constant like any other tenant', async () => {
    const em = fakeEm();
    const wrapped = wrapEmWithTenantContext(em as never, platform);
    await wrapped.findOne('X' as never, {});
    expect(em.calls).toEqual([
      `filter:${platform}`,
      `findOne:${JSON.stringify(platform)}`
    ]);
  });

  it('leaves the EM unwrapped only for undefined', () => {
    const em = fakeEm();
    expect(wrapEmWithTenantContext(em as never, undefined)).toBe(em);
  });
});

describe('bound versus unbound', () => {
  it('tells "nobody bound a tenant" apart from a tenant', async () => {
    expect(getBoundTenantId()).toBeUndefined();
    await withEncryptionContext(org, async () => {
      expect(getBoundTenantId()).toBe(org);
    });
    expect(getBoundTenantId()).toBeUndefined();
  });

  it('creating a wrapped EM never leaks its tenant into the caller', () => {
    expect(getBoundTenantId()).toBeUndefined();
    wrapEmWithTenantContext(fakeEm() as never, org);
    expect(getBoundTenantId()).toBeUndefined();
  });

  it('an org EM binds the org for its calls and nothing else', async () => {
    const em = fakeEm();
    const wrapped = wrapEmWithTenantContext(em as never, org);
    await wrapped.findOne('X' as never, {});
    expect(em.calls).toEqual([
      `filter:${org}`,
      `findOne:${JSON.stringify(org)}`
    ]);
    expect(getBoundTenantId()).toBeUndefined();
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
    // Stepping out of the tenant on purpose is still possible — but the
    // context you step into has to be a real one.
    const em = fakeEm();
    const wrapped = wrapEmWithTenantContext(em as never, org);
    const forked = wrapped.fork();
    await withEncryptionContext(platform, async () => {
      await forked.flush();
    });
    expect(em.calls).toEqual([`filter:${org}`, `flush:"${platform}"`]);
  });
});
