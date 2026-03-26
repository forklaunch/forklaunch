import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const Jwks = defineComplianceEntity({
  name: 'Jwks',
  properties: {
    ...sqlBaseProperties,
    publicKey: fp.string().compliance('none'),
    privateKey: fp.string().compliance('pci')
  }
});

export type Jwks = InferEntity<typeof Jwks>;
