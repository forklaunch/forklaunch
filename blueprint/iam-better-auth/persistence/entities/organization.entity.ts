import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';

export const Organization = defineEntity({
  name: 'Organization',
  properties: {
    ...sqlBaseProperties,
    name: p.string(),
    slug: p.string().unique(),
    logo: p.string().nullable(),
    metadata: p.json<unknown>().nullable(),
    domain: p.string().nullable(),
    subscription: p.string().nullable(),
    status: p.string().nullable()
  }
});

export type Organization = InferEntity<typeof Organization>;
