import { defineEntity, p, type InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { User } from './user.entity';

export const Account = defineEntity({
  name: 'Account',
  properties: {
    ...sqlBaseProperties,
    user: () => p.manyToOne(User),
    accountId: p.string(),
    providerId: p.string(),
    accessToken: p.string().nullable(),
    refreshToken: p.string().nullable(),
    accessTokenExpiresAt: p.datetime().nullable(),
    refreshTokenExpiresAt: p.datetime().nullable(),
    scope: p.string().nullable(),
    idToken: p.string().nullable(),
    password: p.string().nullable()
  }
});

export type Account = InferEntity<typeof Account>;
