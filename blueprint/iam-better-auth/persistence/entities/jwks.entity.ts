import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const jwks = defineEntity({
  name: 'Jwks',
  properties: {
    ...sqlBaseProperties,
    publicKey: p.string(),
    privateKey: p.string()
  }
});

export type Jwks = InferEntity<typeof jwks>;
