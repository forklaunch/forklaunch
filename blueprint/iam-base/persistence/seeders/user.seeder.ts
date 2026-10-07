import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { User as UserEntity } from '../entities/user.entity';
import { user } from '../seed.data';

export class UserSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    const createdUser = em.create(UserEntity, user);
    await em.persist(createdUser).flush();
  }
}
