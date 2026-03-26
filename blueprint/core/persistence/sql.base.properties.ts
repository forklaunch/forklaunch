import { fp } from '@forklaunch/core/persistence';
import { v4 } from 'uuid';

export const sqlBaseProperties = {
  id: fp
    .uuid()
    .primary()
    .onCreate(() => v4())
    .compliance('none'),
  createdAt: fp
    .datetime()
    .onCreate(() => new Date())
    .compliance('none'),
  updatedAt: fp
    .datetime()
    .onCreate(() => new Date())
    .onUpdate(() => new Date())
    .compliance('none'),
  retentionAnonymizedAt: fp.datetime().nullable().compliance('none')
};
