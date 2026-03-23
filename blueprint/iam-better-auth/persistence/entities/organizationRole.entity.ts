import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const OrganizationRole = defineEntity({
  name: 'OrganizationRole',
  properties: {
    ...sqlBaseProperties,
    organizationId: p.string(),
    role: p.string(),
    permission: p.string()
  }
});

export type OrganizationRole = InferEntity<typeof OrganizationRole>;
