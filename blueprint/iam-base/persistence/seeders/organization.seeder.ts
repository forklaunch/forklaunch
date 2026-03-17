import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { organization as organizationEntity } from '../entities/organization.entity';
import { organization } from '../seed.data';

export class OrganizationSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    const createdOrganization = em.create(organizationEntity, organization);
    await em.persist(createdOrganization).flush();
  }
}
