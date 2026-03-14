import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { User } from './user.entity';

export const Session = defineEntity({
  name: 'Session',
  properties: {
    ...sqlBaseProperties,
    user: () => p.manyToOne(User),
    token: p.string(),
    expiresAt: p.datetime(),
    ipAddress: p.string().nullable(),
    userAgent: p.string().nullable()
  }
});

export type ISession = InferEntity<typeof Session>;
