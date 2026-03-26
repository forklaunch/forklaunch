import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const Organization = defineComplianceEntity({
  name: 'Organization',
  properties: {
    ...sqlBaseProperties,
    name: fp.string().compliance('none'),
    slug: fp.string().unique().compliance('none'),
    logo: fp.string().nullable().compliance('none'),
    metadata: fp.json<unknown>().nullable().compliance('none'),
    domain: fp.string().nullable().compliance('none'),
    subscription: fp.string().nullable().compliance('none'),
    status: fp.string().nullable().compliance('none')
  }
});

export type Organization = InferEntity<typeof Organization>;
