import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const Member = defineComplianceEntity({
  name: 'Member',
  properties: {
    ...sqlBaseProperties,
    organizationId: fp.string().compliance('none'),
    userId: fp.string().compliance('none'),
    role: fp.string().default('member').compliance('none')
  }
});

export type Member = InferEntity<typeof Member>;
