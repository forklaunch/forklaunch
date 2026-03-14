import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { Role } from '../entities/role.entity';
import { adminRole, editorRole, systemRole, viewerRole } from '../seed.data';

export class RoleSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    const roles = [
      em.create(Role, viewerRole),
      em.create(Role, editorRole),
      em.create(Role, adminRole),
      em.create(Role, systemRole)
    ];
    await em.persist(roles).flush();
  }
}
