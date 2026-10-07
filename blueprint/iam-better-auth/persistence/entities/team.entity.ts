import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const Team = defineComplianceEntity({
  name: 'Team',
  properties: {
    ...sqlBaseProperties,
    name: fp.string().compliance('none'),
    // better-auth >= 1.7.0 organization plugin tracks and increments a required
    // memberCount on each team (created at 0); without it the mikro-orm adapter
    // throws "Can't find property memberCount on entity Team" on org creation.
    memberCount: fp.integer().default(0).compliance('none'),
    organizationId: fp.string().compliance('none')
  }
});

export type Team = InferEntity<typeof Team>;
