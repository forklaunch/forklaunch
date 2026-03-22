import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const TeamMember = defineEntity({
  name: 'TeamMember',
  properties: {
    ...sqlBaseProperties,
    teamId: p.string(),
    userId: p.string()
  }
});

export type TeamMember = InferEntity<typeof TeamMember>;
