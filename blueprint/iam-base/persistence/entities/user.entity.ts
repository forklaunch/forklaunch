import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineEntity, p } from '@mikro-orm/core';
import { Organization } from './organization.entity';
import { Role } from './role.entity';

export const User = defineEntity({
  name: 'User',
  properties: {
    ...sqlBaseProperties,
    email: p.string().unique(),
    firstName: p.string(),
    lastName: p.string(),
    phoneNumber: p.string().nullable(),
    organization: () => p.manyToOne(Organization).nullable(),
    roles: () => p.manyToMany(Role),
    subscription: p.string().unique().nullable(),
    providerFields: p.json<unknown>().nullable()
  }
});

// export type User = InferEntity<typeof user>;
