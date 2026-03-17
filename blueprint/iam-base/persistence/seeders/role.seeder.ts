import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { role } from '../entities/role.entity';
import { adminRole, editorRole, systemRole, viewerRole } from '../seed.data';

export class RoleSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    const roles = [
      em.create(role, viewerRole),
      em.create(role, editorRole),
      em.create(role, adminRole),
      em.create(role, systemRole)
    ];
    await em.persist(roles).flush();
  }
}
