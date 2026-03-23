import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineEntity, p } from '@mikro-orm/core';

export const Permission = defineEntity({
  name: 'Permission',
  properties: {
    ...sqlBaseProperties,
    slug: p.string()
  }
});
