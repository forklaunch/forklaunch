import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { plan as planEntity } from '../entities/plan.entity';
import { plan } from '../seed.data';

export class PlanSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    const createdPlan = em.create(planEntity, plan);
    await em.persist(createdPlan).flush();
  }
}
