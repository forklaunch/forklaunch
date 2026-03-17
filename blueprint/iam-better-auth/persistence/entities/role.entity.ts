import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { permission } from './permission.entity';

export const role = defineEntity({
  name: 'Role',
  properties: {
    ...sqlBaseProperties,
    name: p.string(),
    permissions: () => p.manyToMany(permission)
  }
});

export type Role = InferEntity<typeof role>;
