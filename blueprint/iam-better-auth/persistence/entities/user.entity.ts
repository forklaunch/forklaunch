import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const User = defineEntity({
  name: 'User',
  properties: {
    ...sqlBaseProperties,
    email: p.string().unique(),
    emailVerified: p.boolean(),
    name: p.string().unique(),
    firstName: p.string(),
    lastName: p.string(),
    image: p.string().nullable(),
    phoneNumber: p.string().nullable(),
    subscription: p.string().unique().nullable(),
    providerFields: p.json<unknown>().nullable()
  }
});

export type User = InferEntity<typeof User>;
