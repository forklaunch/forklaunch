import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';

/**
 * A topic page: a procedure, condition or medication, answered through a
 * question framework. Pages start as 'draft' and are only shown to doctors
 * once a clinician has approved them (approvedBy / approvedAt).
 */
export const Topic = defineComplianceEntity({
  name: 'Topic',
  properties: {
    ...sqlBaseProperties,
    slug: fp.string().unique().compliance('none'),
    // 'procedure' | 'condition' | 'medication'
    topicType: fp.string().compliance('none'),
    title: fp.string().compliance('none'),
    frameworkKey: fp.string().compliance('none'),
    meshDescriptorUi: fp.string().nullable().compliance('none'),
    // names the topic goes by; evidence must name one in full
    searchTerms: fp.string().array().compliance('none'),
    // the topic's own hint words per framework item (a procedure's anatomy,
    // instruments, complications), added to the framework's
    searchHints: fp.json<Record<string, string[]>>().nullable().compliance('none'),
    // 'draft' | 'approved'
    status: fp.string().compliance('none'),
    approvedBy: fp.string().nullable().compliance('none'),
    approvedAt: fp.datetime().nullable().compliance('none'),
    assembledAt: fp.datetime().nullable().compliance('none')
  }
});

export type Topic = InferEntity<typeof Topic>;
