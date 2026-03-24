import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { Organization } from './organization.entity';
import { Role } from './role.entity';

export const User = defineComplianceEntity({
  name: 'User',
  properties: {
    ...sqlBaseProperties,
    email: fp.string().unique().compliance('none'),
    firstName: fp.string().compliance('none'),
    lastName: fp.string().compliance('none'),
    phoneNumber: fp.string().nullable().compliance('none'),
    organization: () => fp.manyToOne(Organization).nullable(),
    roles: () => fp.manyToMany(Role),
    subscription: fp.string().unique().nullable().compliance('none'),
    providerFields: fp.json<unknown>().nullable().compliance('none')
  }
});

// export type User = InferEntity<typeof user>;
