import {
  FieldEncryptor,
  getCurrentTenantId,
  withEncryptionContext
} from '@forklaunch/core/persistence';
import { describe, expect, it } from 'vitest';
import {
  AUTH_ENCRYPTION_POLICY,
  AUTH_ENCRYPTION_CONTEXT,
  createAuthEncryptionOrm,
  withAuthEncryptionPolicy
} from '../domain/utils/authEncryptionPolicy.util';
function fixture() {
  const seen: string[] = [];
  const rows: { id: string; encryptionPolicy?: string }[] = [];
  const orm = {
    getMetadata: () => ({ find: (name: unknown) => ({ className: name }) }),
    em: {
      fork: () => ({ find: async () => rows }),
      create: (_model: unknown, data: unknown) => data,
      findOne: async (..._args: unknown[]) => {
        seen.push(getCurrentTenantId());
        return null;
      },
      nativeUpdate: async (..._args: unknown[]) => 1,
      flush: async () => {
        seen.push(getCurrentTenantId());
      },
      transactional: async (callback: () => Promise<void>) => {
        await callback();
        seen.push(getCurrentTenantId());
      }
    }
  };
  return { orm: createAuthEncryptionOrm(orm), seen, rows };
}
describe('fresh iam-service-v1 policy', () => {
  it('keeps app keys and business namespaces cryptographically separate', () => {
    const appA = new FieldEncryptor('synthetic-app-a-master-key-long-enough');
    const appB = new FieldEncryptor('synthetic-app-b-master-key-long-enough');
    const encrypted = appA.encrypt(
      'synthetic-secret',
      AUTH_ENCRYPTION_CONTEXT
    )!;
    expect(encrypted).not.toContain('synthetic-secret');
    expect(appA.decrypt(encrypted, AUTH_ENCRYPTION_CONTEXT)).toBe(
      'synthetic-secret'
    );
    expect(() => appA.decrypt(encrypted, 'organization-a')).toThrow();
    expect(() => appB.decrypt(encrypted, AUTH_ENCRYPTION_CONTEXT)).toThrow();
  });
  it('marks ciphertext and requires explicit scope', () => {
    const f = fixture();
    expect(AUTH_ENCRYPTION_POLICY).toBe('iam-service-v1');
    expect(() => f.orm.em.create('Account', {})).toThrow('scope');
    expect(
      withAuthEncryptionPolicy(() => f.orm.em.create('Account', { user: 'a' }))
    ).toEqual({ user: 'a', encryptionPolicy: AUTH_ENCRYPTION_POLICY });
  });
  it('covers account, session flush and signing key operations through transaction commit', async () => {
    const f = fixture();
    f.rows.push({ id: 'a', encryptionPolicy: AUTH_ENCRYPTION_POLICY });
    await withAuthEncryptionPolicy(() =>
      f.orm.em.transactional(async () => {
        await f.orm.em.findOne('Account', { id: 'a' });
        f.orm.em.create('Session', {});
        await f.orm.em.flush();
        f.orm.em.create('Jwks', {});
        await f.orm.em.flush();
      })
    );
    expect(f.seen).toEqual(Array(4).fill(AUTH_ENCRYPTION_CONTEXT));
    expect(getCurrentTenantId()).toBe('');
  });
  it('refuses legacy ciphertext before hydration', async () => {
    const f = fixture();
    f.rows.push({ id: 'old' });
    await expect(
      withAuthEncryptionPolicy(() => f.orm.em.findOne('Account', { id: 'old' }))
    ).rejects.toThrow('migration');
    expect(f.seen).toEqual([]);
  });
  it('refuses metadata rewrites on auth rows without restricting unrelated model fields', async () => {
    const f = fixture();
    await expect(
      withAuthEncryptionPolicy(() =>
        f.orm.em.nativeUpdate('Account', {}, { encryptionPolicy: 'legacy' })
      )
    ).rejects.toThrow('migration');
    await expect(
      f.orm.em.nativeUpdate('OtherModel', {}, { encryptionPolicy: 'unrelated' })
    ).resolves.toBe(1);
  });
  it('does not replace business tenant contexts or leak into concurrent work', async () => {
    const f = fixture();
    await Promise.all([
      withAuthEncryptionPolicy(async () => {
        await Promise.resolve();
        expect(getCurrentTenantId()).toBe(AUTH_ENCRYPTION_CONTEXT);
      }),
      withEncryptionContext('organization-a', async () => {
        await Promise.resolve();
        expect(getCurrentTenantId()).toBe('organization-a');
        expect(() => f.orm.em.create('Account', {})).toThrow('incompatible');
      })
    ]);
  });
});
