import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { Session as SessionEntity } from '../entities/session.entity';
import { session } from '../seed.data';

export class SessionSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    const createdSession = em.create(SessionEntity, session);
    await em.persist(createdSession).flush();
  }
}
