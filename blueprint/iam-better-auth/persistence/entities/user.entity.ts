import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const User = defineComplianceEntity({
  name: 'User',
  properties: {
    ...sqlBaseProperties,
    email: fp.string().unique().compliance('none'),
    emailVerified: fp.boolean().compliance('none'),
    name: fp.string().compliance('none'),
    firstName: fp.string().compliance('none'),
    lastName: fp.string().compliance('none'),
    image: fp.string().nullable().compliance('none'),
    phoneNumber: fp.string().nullable().compliance('none'),
    subscription: fp.string().unique().nullable().compliance('none'),
    providerFields: fp.json<unknown>().nullable().compliance('none')
  }
});

export type User = InferEntity<typeof User>;
