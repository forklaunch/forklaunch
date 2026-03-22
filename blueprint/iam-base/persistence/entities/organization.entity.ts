import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineEntity, p } from '@mikro-orm/core';
import { OrganizationStatus } from '../../domain/enum/organizationStatus.enum';
import { User } from './user.entity';

export const Organization = defineEntity({
  name: 'Organization',
  properties: {
    ...sqlBaseProperties,
    name: p.string(),
    users: () => p.oneToMany(User).mappedBy('organization'),
    domain: p.string(),
    logoUrl: p.string().nullable(),
    subscription: p.string().unique(),
    providerFields: p.json().nullable(),
    status: p.enum(() => OrganizationStatus).default(OrganizationStatus.ACTIVE)
  }
});
