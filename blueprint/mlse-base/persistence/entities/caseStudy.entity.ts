import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { Document } from './document.entity';
import { Topic } from './topic.entity';

/**
 * A published case report linked to a topic, grouped under the diagnosis it
 * concerns. The four parts are copied from the report's own sections (never
 * written), limited to what its license allows, and case evidence is always
 * shown as low-level evidence.
 */
export const CaseStudy = defineComplianceEntity({
  name: 'CaseStudy',
  properties: {
    ...sqlBaseProperties,
    topic: () => fp.manyToOne(Topic),
    document: () => fp.manyToOne(Document),
    // the diagnosis heading the case is grouped under
    diagnosis: fp.string().compliance('none'),
    // 'mesh' | 'terms': why the case was judged relevant
    relevanceReason: fp.string().compliance('none'),
    presentation: fp.string().nullable().compliance('none'),
    diagnosisText: fp.string().nullable().compliance('none'),
    management: fp.string().nullable().compliance('none'),
    outcome: fp.string().nullable().compliance('none')
  }
});

export type CaseStudy = InferEntity<typeof CaseStudy>;
