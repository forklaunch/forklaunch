import { p } from '@mikro-orm/core';
import { ObjectId } from '@mikro-orm/mongodb';

export const nosqlBaseProperties = {
  _id: p.type(ObjectId).primary(),
  id: p.string().serializedPrimaryKey().persist(false),
  createdAt: p.datetime().onCreate(() => new Date()),
  updatedAt: p
    .datetime()
    .onCreate(() => new Date())
    .onUpdate(() => new Date())
};
