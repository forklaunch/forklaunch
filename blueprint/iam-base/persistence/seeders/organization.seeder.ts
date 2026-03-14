import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { Organization } from '../entities/organization.entity';
import { organization } from '../seed.data';

export class OrganizationSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    const createdOrganization = em.create(Organization, organization);
    await em.persist(createdOrganization).flush();
  }
}
