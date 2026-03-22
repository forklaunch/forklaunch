import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const Invitation = defineEntity({
  name: 'Invitation',
  properties: {
    ...sqlBaseProperties,
    organizationId: p.string(),
    email: p.string(),
    role: p.string().nullable(),
    status: p.string().default('pending'),
    inviterId: p.string(),
    teamId: p.string().nullable(),
    expiresAt: p.datetime()
  }
});

export type Invitation = InferEntity<typeof Invitation>;
