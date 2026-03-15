import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { Permission } from './permission.entity';

export const Role = defineEntity({
  name: 'Role',
  properties: {
    ...sqlBaseProperties,
    name: p.string(),
    permissions: () => p.manyToMany(Permission)
  }
});

export type IRole = InferEntity<typeof Role>;
