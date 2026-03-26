import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { Permission } from './permission.entity';

export const Role = defineComplianceEntity({
  name: 'Role',
  properties: {
    ...sqlBaseProperties,
    name: fp.string().compliance('none'),
    permissions: () => fp.manyToMany(Permission)
  }
});

// export type Role = InferEntity<typeof role>;
