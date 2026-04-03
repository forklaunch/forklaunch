import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const TeamMember = defineComplianceEntity({
  name: 'TeamMember',
  properties: {
    ...sqlBaseProperties,
    teamId: fp.string().compliance('none'),
    userId: fp.string().compliance('none')
  }
});

export type TeamMember = InferEntity<typeof TeamMember>;
