import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const Verification = defineComplianceEntity({
  name: 'Verification',
  properties: {
    ...sqlBaseProperties,
    identifier: fp.string().compliance('none'),
    value: fp.string().compliance('none'),
    expiresAt: fp.datetime().compliance('none')
  }
});

export type Verification = InferEntity<typeof Verification>;
