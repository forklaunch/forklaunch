import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { CheckoutSession } from '../entities/checkoutSession.entity';
import { checkoutSession } from '../seed.data';

export class CheckoutSessionSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    const createdCheckoutSession = em.create(CheckoutSession, checkoutSession);
    await em.persist(createdCheckoutSession).flush();
  }
}
