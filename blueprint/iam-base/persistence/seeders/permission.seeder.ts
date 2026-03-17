import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { permission } from '../entities/permission.entity';
import { platformReadPermission, platformWritePermission } from '../seed.data';

export class PermissionSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    const permissions = [
      em.create(permission, platformReadPermission),
      em.create(permission, platformWritePermission)
    ];
    await em.persist(permissions).flush();
  }
}
