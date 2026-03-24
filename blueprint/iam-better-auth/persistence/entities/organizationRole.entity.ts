import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const OrganizationRole = defineComplianceEntity({
  name: 'OrganizationRole',
  properties: {
    ...sqlBaseProperties,
    organizationId: fp.string().compliance('none'),
    role: fp.string().compliance('none'),
    permission: fp.string().compliance('none')
  }
});

export type OrganizationRole = InferEntity<typeof OrganizationRole>;
