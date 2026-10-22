import { describe, expect, it, vi } from 'vitest';
import { defineEntity, MikroORM, p } from '@mikro-orm/sqlite';
import { createAuthEncryptionOrm, withAuthEncryptionPolicy } from '../../blueprint/iam-better-auth/domain/utils/authEncryptionPolicy.util';

// Repository regression only: do not emit SQLite imports into PostgreSQL apps.
describe('fresh IAM policy transaction snapshot', () => {
  it('checks policy metadata in the active database transaction', async () => {
    const account = defineEntity({
      name: 'Account',
      properties: {
        id: p.integer().primary(),
        encryptionPolicy: p.string().nullable()
      }
    });
    class Account extends account.class {}
    account.setClass(Account);
    const raw = await MikroORM.init({
      entities: [Account],
      dbName: ':memory:'
    });
    try {
      await raw.schema.refresh();
      const orm = createAuthEncryptionOrm(raw);
      await withAuthEncryptionPolicy(() =>
        orm.em.transactional(async () => {
          // Simulate a legacy row encountered before this transaction commits.
          await raw.em.getConnection().execute(
            'insert into account (id, encryption_policy) values (?, ?)',
            [1, 'legacy'],
            'run',
            raw.em.getContext().getTransactionContext()
          );
          const transaction = raw.em.getContext().getTransactionContext();
          expect(transaction).toBeDefined();
          expect(raw.em.fork().getTransactionContext()).toBeUndefined();
          expect(
            raw.em.fork({ keepTransactionContext: true }).getTransactionContext()
          ).toBe(transaction);
          await expect(orm.em.findOne(Account, { id: 1 })).rejects.toThrow(
            'migration'
          );
        })
      );
    } finally {
      await raw.close(true);
    }
  });
  it('checks every signing-key marker without limiting valid rotation history', async () => {
    const definition = defineEntity({
      name: 'Jwks',
      properties: {
        id: p.integer().primary(),
        encryptionPolicy: p.string().nullable()
      }
    });
    class Jwks extends definition.class {}
    definition.setClass(Jwks);
    const raw = await MikroORM.init({ entities: [Jwks], dbName: ':memory:' });
    try {
      await raw.schema.refresh();
      await raw.em
        .getConnection()
        .execute(
          'insert into jwks (id, encryption_policy) values ' +
            Array(1001).fill('(?, ?)').join(', '),
          Array.from({ length: 1001 }, (_, id) => [
            id + 1,
            'iam-service-v1'
          ]).flat()
        );
      const secretReads = vi.spyOn(raw.em, 'find');
      const orm = createAuthEncryptionOrm(raw);
      await withAuthEncryptionPolicy(() =>
        orm.em.transactional(async () => {
          await expect(orm.em.find(Jwks, {})).resolves.toHaveLength(1001);
        })
      );
      expect(secretReads).toHaveBeenCalledTimes(1);
      // The invalid record is outside the former 1,001-row inspection page and
      // visible only inside this transaction until commit. Both NULL and unknown
      // markers must fail before the actual protected read is invoked.
      for (const marker of [null, 'legacy']) {
        await withAuthEncryptionPolicy(() =>
          orm.em.transactional(async () => {
            const transaction = raw.em.getContext().getTransactionContext();
            await raw.em
              .getConnection()
              .execute(
                'insert into jwks (id, encryption_policy) values (?, ?)',
                [2000, marker],
                'run',
                transaction
              );
            await expect(orm.em.find(Jwks, {})).rejects.toThrow('migration');
            await raw.em
              .getConnection()
              .execute(
                'delete from jwks where id = ?',
                [2000],
                'run',
                transaction
              );
          })
        );
      }
      expect(secretReads).toHaveBeenCalledTimes(1);
      secretReads.mockRestore();
    } finally {
      await raw.close(true);
    }
  });
});
