import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { Verification as VerificationEntity } from '../entities/verification.entity';
import { verification } from '../seed.data';

export class VerificationSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    const createdVerification = em.create(VerificationEntity, verification);
    await em.persist(createdVerification).flush();
  }
}
