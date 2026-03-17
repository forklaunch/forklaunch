import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { billingProvider as billingProviderEntity } from '../entities/billingProvider.entity';
import { billingProvider } from '../seed.data';

export class BillingProviderSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    const createdBillingProvider = em.create(
      billingProviderEntity,
      billingProvider
    );
    await em.persist(createdBillingProvider).flush();
  }
}
