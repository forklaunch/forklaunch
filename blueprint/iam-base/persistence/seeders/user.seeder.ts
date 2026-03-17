import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { user as userEntity } from '../entities/user.entity';
import { user } from '../seed.data';

export class UserSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    const createdUser = em.create(userEntity, user);
    await em.persist(createdUser).flush();
  }
}
