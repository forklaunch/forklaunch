import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { verification as verificationEntity } from '../entities/verification.entity';
import { verification } from '../seed.data';

export class VerificationSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    const createdVerification = em.create(verificationEntity, verification);
    await em.persist(createdVerification).flush();
  }
}
