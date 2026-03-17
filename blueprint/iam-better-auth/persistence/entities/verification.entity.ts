import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const verification = defineEntity({
  name: 'Verification',
  properties: {
    ...sqlBaseProperties,
    identifier: p.string(),
    value: p.string(),
    expiresAt: p.datetime()
  }
});

export type Verification = InferEntity<typeof verification>;
