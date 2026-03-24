import { fp } from '@forklaunch/core/persistence';
import { ObjectId } from '@mikro-orm/mongodb';

export const nosqlBaseProperties = {
  _id: fp.type(ObjectId).primary().compliance('none'),
  id: fp.string().serializedPrimaryKey().persist(false).compliance('none'),
  createdAt: fp
    .datetime()
    .onCreate(() => new Date())
    .compliance('none'),
  updatedAt: fp
    .datetime()
    .onCreate(() => new Date())
    .onUpdate(() => new Date())
    .compliance('none')
};
