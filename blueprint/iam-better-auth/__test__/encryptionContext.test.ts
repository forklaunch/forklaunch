import { getCurrentTenantId } from '@forklaunch/core/persistence';
import { describe, expect, it, vi } from 'vitest';
import {
  createEncryptionAwareOrm,
  getLatchedEncryptionContext,
  latchEncryptionContext,
  withEncryptionContextLatch
} from '../domain/utils/encryptionContext.util';

/**
 * `registrations.ts` wires the service's own EntityManager through
 * `wrapEmWithTenantContext`, so application code reads and writes under the
 * request's tenant. Better Auth used to get the raw ORM and therefore read
 * under no tenant at all.
 *
 * That is invisible in a single-tenant deployment — both sides use `''` — and
 * breaks the moment a tenant id is supplied, because `FieldEncryptor` derives
 * its key from the tenant. The encrypted columns on `account` (`password`,
 * `accessToken`, `refreshToken`, `idToken`) then fail with "ciphertext is
 * corrupted or the wrong key was used", surfacing as a 500 from sign-in.
 *
 * These assert on the tenant the EntityManager actually sees, since that is
 * the input to key derivation.
 */

const buildOrm = (seen: { method: string; tenant: string }[]) => {
  const record =
    (method: string) =>
    async (..._args: unknown[]) => {
      seen.push({ method, tenant: getCurrentTenantId() });
      return null;
    };

  return {
    em: {
      findOne: vi.fn(record('findOne')),
      findOneOrFail: vi.fn(record('findOneOrFail')),
      find: vi.fn(record('find')),
      findAll: vi.fn(record('findAll')),
      findAndCount: vi.fn(record('findAndCount')),
      persist: vi.fn(),
      flush: vi.fn()
    }
  };
};

describe('encryption context latch', () => {
  it('reports nothing latched outside a scope', () => {
    expect(getLatchedEncryptionContext()).toBeUndefined();
  });

  it('carries the tenant to later reads in the same request', () => {
    withEncryptionContextLatch(() => {
      latchEncryptionContext('tenant-1');
      expect(getLatchedEncryptionContext()).toBe('tenant-1');
    });
  });

  it('keeps the no-tenant context distinct from nothing latched', () => {
    withEncryptionContextLatch(() => {
      latchEncryptionContext('');
      expect(getLatchedEncryptionContext()).toBe('');
      expect(getLatchedEncryptionContext()).not.toBeUndefined();
    });
  });

  it('keeps the first answer when a later call disagrees', () => {
    withEncryptionContextLatch(() => {
      latchEncryptionContext('tenant-1');
      latchEncryptionContext('tenant-2');
      expect(getLatchedEncryptionContext()).toBe('tenant-1');
    });
  });

  it('does not leak between concurrent requests', async () => {
    const request = (tenant: string, delayMs: number) =>
      withEncryptionContextLatch(async () => {
        latchEncryptionContext(tenant);
        await new Promise((resolve) => setTimeout(resolve, delayMs));
        return getLatchedEncryptionContext();
      });

    const [a, b] = await Promise.all([
      request('tenant-a', 20),
      request('tenant-b', 1)
    ]);

    expect(a).toBe('tenant-a');
    expect(b).toBe('tenant-b');
  });

  it('survives the awaits between lookup and hydration', async () => {
    await withEncryptionContextLatch(async () => {
      latchEncryptionContext('tenant-1');
      await new Promise((resolve) => setImmediate(resolve));
      await new Promise((resolve) => setTimeout(resolve, 5));
      expect(getLatchedEncryptionContext()).toBe('tenant-1');
    });
  });

  it('ends with the request', async () => {
    await withEncryptionContextLatch(async () => {
      latchEncryptionContext('tenant-1');
    });
    expect(getLatchedEncryptionContext()).toBeUndefined();
  });
});

describe('createEncryptionAwareOrm', () => {
  it.each([
    'findOne',
    'findOneOrFail',
    'find',
    'findAll',
    'findAndCount'
  ] as const)('runs %s under the latched tenant', async (method) => {
    const seen: { method: string; tenant: string }[] = [];
    const orm = buildOrm(seen);
    const wrapped = createEncryptionAwareOrm(orm);

    await withEncryptionContextLatch(async () => {
      latchEncryptionContext('tenant-1');
      await (
        wrapped.em as unknown as Record<
          string,
          (...args: unknown[]) => Promise<unknown>
        >
      )[method]('account', { id: 'a1' });
    });

    // Before the fix every one of these saw '' and could not decrypt a row
    // written under 'tenant-1'.
    expect(seen).toEqual([{ method, tenant: 'tenant-1' }]);
  });

  it('leaves reads outside a Better Auth request untouched', async () => {
    const seen: { method: string; tenant: string }[] = [];
    const orm = buildOrm(seen);
    const wrapped = createEncryptionAwareOrm(orm);

    await wrapped.em.findOne('account', { id: 'a1' });

    expect(seen).toEqual([{ method: 'findOne', tenant: '' }]);
  });

  it('applies nothing before a tenant is known', async () => {
    const seen: { method: string; tenant: string }[] = [];
    const orm = buildOrm(seen);
    const wrapped = createEncryptionAwareOrm(orm);

    await withEncryptionContextLatch(async () => {
      await wrapped.em.findOne('account', { id: 'a1' });
    });

    expect(seen).toEqual([{ method: 'findOne', tenant: '' }]);
  });

  it('passes writes and unit-of-work methods straight through', async () => {
    const seen: { method: string; tenant: string }[] = [];
    const orm = buildOrm(seen);
    const wrapped = createEncryptionAwareOrm(orm);

    await wrapped.em.flush();
    expect(orm.em.flush).toHaveBeenCalled();
    expect(seen).toEqual([]);
  });

  it('preserves the ORM type, so callers keep their driver', () => {
    const orm = buildOrm([]);
    const wrapped = createEncryptionAwareOrm(orm);
    // Constraining to `MikroORM` instead of `{ em }` made the driver-specific
    // ORMs unassignable (readonly `~entities`), which is why the generic is
    // structural.
    expect(wrapped).toBeDefined();
    expect(typeof wrapped.em.findOne).toBe('function');
  });
});
