import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const Team = defineComplianceEntity({
  name: 'Team',
  properties: {
    ...sqlBaseProperties,
    name: fp.string().compliance('none'),
    organizationId: fp.string().compliance('none')
  }
});

export type Team = InferEntity<typeof Team>;
