import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { User } from '../entities/user.entity';
import { user } from '../seed.data';

export class UserSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    const createdUser = em.create(User, user);
    await em.persist(createdUser).flush();
  }
}
