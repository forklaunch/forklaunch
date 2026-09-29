import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';

/**
 * A search a doctor saved to run again. The query is encrypted at rest and
 * included in the user's data export and erasure.
 */
export const SavedSearch = defineComplianceEntity({
  name: 'SavedSearch',
  userIdField: 'userId',
  properties: {
    ...sqlBaseProperties,
    organizationId: fp.string().compliance('none'),
    userId: fp.string().compliance('none'),
    name: fp.string().compliance('pii'),
    query: fp.string().compliance('pii'),
    topicSlug: fp.string().nullable().compliance('none')
  }
});

export type SavedSearch = InferEntity<typeof SavedSearch>;
