import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { PaymentLink } from '../entities/paymentLink.entity';
import { paymentLink } from '../seed.data';

export class PaymentLinkSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    const createdPaymentLink = em.create(PaymentLink, paymentLink);
    await em.persist(createdPaymentLink).flush();
  }
}
