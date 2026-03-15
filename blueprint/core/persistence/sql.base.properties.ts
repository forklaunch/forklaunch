import { p } from '@mikro-orm/core';
import { v4 } from 'uuid';

export const sqlBaseProperties = {
  id: p
    .uuid()
    .primary()
    .onCreate(() => v4()),
  createdAt: p.datetime().onCreate(() => new Date()),
  updatedAt: p
    .datetime()
    .onCreate(() => new Date())
    .onUpdate(() => new Date())
};
