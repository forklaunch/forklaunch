import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';
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
    status: p.enum(() => OrganizationStatus).default(OrganizationStatus.ACTIVE)
  }
});

export type IOrganization = InferEntity<typeof Organization>;
