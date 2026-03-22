import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineEntity, p } from '@mikro-orm/core';
import { Permission } from './permission.entity';

export const Role = defineEntity({
  name: 'Role',
  properties: {
    ...sqlBaseProperties,
    name: p.string(),
    permissions: () => p.manyToMany(Permission)
  }
});

// export type Role = InferEntity<typeof role>;
