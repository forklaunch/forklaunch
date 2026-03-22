import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const Team = defineEntity({
  name: 'Team',
  properties: {
    ...sqlBaseProperties,
    name: p.string(),
    organizationId: p.string()
  }
});

export type Team = InferEntity<typeof Team>;
