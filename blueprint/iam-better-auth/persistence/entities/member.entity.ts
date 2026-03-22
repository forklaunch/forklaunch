import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const Member = defineEntity({
  name: 'Member',
  properties: {
    ...sqlBaseProperties,
    organizationId: p.string(),
    userId: p.string(),
    role: p.string().default('member')
  }
});

export type Member = InferEntity<typeof Member>;
