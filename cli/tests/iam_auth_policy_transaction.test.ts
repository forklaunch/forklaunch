import { describe, expect, it } from 'vitest';
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
});
