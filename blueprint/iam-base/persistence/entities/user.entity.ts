import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { organization } from './organization.entity';
import { role } from './role.entity';

export const user = defineEntity({
  name: 'User',
  properties: {
    ...sqlBaseProperties,
    email: p.string().unique(),
    firstName: p.string(),
    lastName: p.string(),
    phoneNumber: p.string().nullable(),
    organization: () => p.manyToOne(organization).nullable(),
    roles: () => p.manyToMany(role),
    subscription: p.string().unique().nullable(),
    providerFields: p.json<unknown>().nullable()
  }
});

export type User = InferEntity<typeof user>;
