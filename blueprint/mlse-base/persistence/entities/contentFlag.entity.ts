import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { Document } from './document.entity';

/**
 * A reviewer's report that a document is wrong, outdated or unsafe to show.
 * While a flag is open the document is left out of search, answers and topic
 * pages; a reviewer then resolves the flag (document corrected or removed)
 * or rejects it (the document is fine).
 */
export const ContentFlag = defineComplianceEntity({
  name: 'ContentFlag',
  properties: {
    ...sqlBaseProperties,
    document: () => fp.manyToOne(Document),
    reason: fp.string().compliance('none'),
    // 'open' | 'resolved' | 'rejected'
    status: fp.string().compliance('none'),
    flaggedBy: fp.string().compliance('none'),
    resolvedBy: fp.string().nullable().compliance('none'),
    resolution: fp.string().nullable().compliance('none'),
    resolvedAt: fp.datetime().nullable().compliance('none')
  }
});

export type ContentFlag = InferEntity<typeof ContentFlag>;
