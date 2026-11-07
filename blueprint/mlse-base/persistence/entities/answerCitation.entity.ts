import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { GeneratedAnswer } from './generatedAnswer.entity';

/**
 * A source an answer cited, copied at answer time so the audit trail still
 * shows what a doctor saw after the document is updated or retracted.
 */
export const AnswerCitation = defineComplianceEntity({
  name: 'AnswerCitation',
  properties: {
    ...sqlBaseProperties,
    answer: () => fp.manyToOne(GeneratedAnswer),
    passageId: fp.string().compliance('none'),
    // 'corpus' | 'live'
    origin: fp.string().compliance('none'),
    sourceKey: fp.string().compliance('none'),
    externalId: fp.string().compliance('none'),
    title: fp.string().compliance('none'),
    url: fp.string().compliance('none'),
    sectionPath: fp.string().compliance('none'),
    licenseScope: fp.string().compliance('none')
  }
});

export type AnswerCitation = InferEntity<typeof AnswerCitation>;
