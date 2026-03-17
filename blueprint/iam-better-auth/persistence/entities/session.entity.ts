import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { user } from './user.entity';

export const session = defineEntity({
  name: 'Session',
  properties: {
    ...sqlBaseProperties,
    user: () => p.manyToOne(user),
    token: p.string(),
    expiresAt: p.datetime(),
    ipAddress: p.string().nullable(),
    userAgent: p.string().nullable()
  }
});

export type Session = InferEntity<typeof session>;
