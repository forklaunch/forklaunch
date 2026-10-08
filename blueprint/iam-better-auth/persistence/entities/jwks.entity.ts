import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const Jwks = defineComplianceEntity({
  name: 'Jwks',
  properties: {
    ...sqlBaseProperties,
    publicKey: fp.string().compliance('none'),
    privateKey: fp.string().compliance('pci'),
    // better-auth >= 1.7.0 JWKS fields (key expiry + algorithm/curve metadata).
    expiresAt: fp.datetime().nullable().compliance('none'),
    alg: fp.string().nullable().compliance('none'),
    crv: fp.string().nullable().compliance('none')
  }
});

export type Jwks = InferEntity<typeof Jwks>;
