import { withEncryptionContext } from '@forklaunch/core/persistence';
import {
  AUTH_ENCRYPTION_CONTEXT,
  AUTH_ENCRYPTION_POLICY
} from '../../domain/utils/authEncryptionPolicy.util';
import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { Account as AccountEntity } from '../entities/account.entity';
import { account } from '../seed.data';

export class AccountSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    await withEncryptionContext(AUTH_ENCRYPTION_CONTEXT, async () => {
      const createdAccount = em.create(AccountEntity, {
        ...account,
        encryptionPolicy: AUTH_ENCRYPTION_POLICY
      });
      await em.persist(createdAccount).flush();
    });
  }
}
