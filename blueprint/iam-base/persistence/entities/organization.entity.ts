import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { OrganizationStatus } from '../../domain/enum/organizationStatus.enum';
import { User } from './user.entity';

export const Organization = defineComplianceEntity({
  name: 'Organization',
  properties: {
    ...sqlBaseProperties,
    name: fp.string().compliance('none'),
    users: () => fp.oneToMany(User).mappedBy('organization'),
    domain: fp.string().compliance('none'),
    logoUrl: fp.string().nullable().compliance('none'),
    subscription: fp.string().unique().compliance('none'),
    providerFields: fp.json().nullable().compliance('none'),
    status: fp
      .enum(() => OrganizationStatus)
      .default(OrganizationStatus.ACTIVE)
      .compliance('none')
  }
});
