import {
  defineComplianceEntity,
  fp,
  RetentionDuration
} from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { User } from './user.entity';

export const Account = defineComplianceEntity({
  name: 'Account',
  retention: {
    duration: RetentionDuration.years(7),
    action: 'anonymize'
  },
  properties: {
    ...sqlBaseProperties,
    user: () => fp.manyToOne(User),
    accountId: fp.string().compliance('none'),
    providerId: fp.string().compliance('none'),
    accessToken: fp.string().nullable().compliance('pci'),
    refreshToken: fp.string().nullable().compliance('pci'),
    accessTokenExpiresAt: fp.datetime().nullable().compliance('none'),
    refreshTokenExpiresAt: fp.datetime().nullable().compliance('none'),
    scope: fp.string().nullable().compliance('none'),
    idToken: fp.string().nullable().compliance('pci'),
    password: fp.string().nullable().compliance('pci')
  }
});

export type Account = InferEntity<typeof Account>;
