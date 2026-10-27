import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import {
  defineComplianceEntity,
  fp,
  RetentionDuration
} from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';

/**
 * A doctor's recent searches. The query is encrypted at rest, is not stored
 * at all for queries classified as patient-specific, prescription or
 * emergency, and is removed by the retention job after 90 days (the entry
 * stays, anonymized, for usage counts).
 */
export const SearchHistory = defineComplianceEntity({
  name: 'SearchHistory',
  userIdField: 'userId',
  retention: {
    duration: RetentionDuration.days(90),
    action: 'anonymize'
  },
  properties: {
    ...sqlBaseProperties,
    organizationId: fp.string().compliance('none'),
    userId: fp.string().compliance('none'),
    query: fp.string().nullable().compliance('pii'),
    queryClass: fp.string().compliance('none'),
    // 'search' | 'answer' | 'voice'
    channel: fp.string().compliance('none'),
    answerId: fp.string().nullable().compliance('none')
  }
});

export type SearchHistory = InferEntity<typeof SearchHistory>;
