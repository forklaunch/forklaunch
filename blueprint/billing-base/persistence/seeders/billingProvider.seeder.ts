import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { BillingProvider } from '../entities/billingProvider.entity';
import { billingProvider } from '../seed.data';

export class BillingProviderSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    const createdBillingProvider = em.create(BillingProvider, billingProvider);
    await em.persist(createdBillingProvider).flush();
  }
}
