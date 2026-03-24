import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { User } from './user.entity';

export const Session = defineComplianceEntity({
  name: 'Session',
  properties: {
    ...sqlBaseProperties,
    user: () => fp.manyToOne(User),
    token: fp.string().compliance('none'),
    expiresAt: fp.datetime().compliance('none'),
    ipAddress: fp.string().nullable().compliance('none'),
    userAgent: fp.string().nullable().compliance('none'),
    activeOrganizationId: fp.string().nullable().compliance('none'),
    activeTeamId: fp.string().nullable().compliance('none')
  }
});

export type Session = InferEntity<typeof Session>;
