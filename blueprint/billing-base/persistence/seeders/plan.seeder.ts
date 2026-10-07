import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { Plan } from '../entities/plan.entity';
import { plan } from '../seed.data';

export class PlanSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    const createdPlan = em.create(Plan, plan);
    await em.persist(createdPlan).flush();
  }
}
