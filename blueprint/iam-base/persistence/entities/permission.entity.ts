import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';

export const Permission = defineComplianceEntity({
  name: 'Permission',
  properties: {
    ...sqlBaseProperties,
    slug: fp.string().compliance('none')
  }
});
