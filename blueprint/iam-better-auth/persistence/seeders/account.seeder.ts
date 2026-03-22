import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { Account as AccountEntity } from '../entities/account.entity';
import { account } from '../seed.data';

export class AccountSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    const createdAccount = em.create(AccountEntity, account);
    await em.persist(createdAccount).flush();
  }
}
