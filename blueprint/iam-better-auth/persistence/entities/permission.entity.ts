import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const Permission = defineEntity({
  name: 'Permission',
  properties: {
    ...sqlBaseProperties,
    slug: p.string()
  }
});

export type IPermission = InferEntity<typeof Permission>;
