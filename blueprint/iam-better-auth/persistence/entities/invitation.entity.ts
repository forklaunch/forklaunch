import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const Invitation = defineComplianceEntity({
  name: 'Invitation',
  properties: {
    ...sqlBaseProperties,
    organizationId: fp.string().compliance('none'),
    email: fp.string().compliance('none'),
    role: fp.string().nullable().compliance('none'),
    status: fp.string().default('pending').compliance('none'),
    inviterId: fp.string().compliance('none'),
    teamId: fp.string().nullable().compliance('none'),
    expiresAt: fp.datetime().compliance('none')
  }
});

export type Invitation = InferEntity<typeof Invitation>;
