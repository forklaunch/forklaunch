import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { AnswerSectionDto } from '@forklaunch/interfaces-mlse/types';
import type { InferEntity } from '@mikro-orm/core';

export type RemovedSentenceRecord = {
  sectionKey: string;
  text: string;
  reason: string;
};

/**
 * Audit record of one answer: how the query was classified, which provider
 * and model drafted it, the verified sections exactly as shown, and every
 * drafted sentence verification removed, with the reason.
 *
 * The query text is stored only for literature questions. For queries
 * classified as patient-specific, prescription or emergency it is not
 * stored, so details of a patient typed into the search never reach the
 * database.
 */
export const GeneratedAnswer = defineComplianceEntity({
  name: 'GeneratedAnswer',
  properties: {
    ...sqlBaseProperties,
    query: fp.string().nullable().compliance('none'),
    queryClass: fp.string().compliance('none'),
    classificationReason: fp.string().compliance('none'),
    // 'answer' | 'label_range' | 'boundary' | 'emergency' | 'source_not_found'
    kind: fp.string().compliance('none'),
    topicSlug: fp.string().nullable().compliance('none'),
    provider: fp.string().nullable().compliance('none'),
    model: fp.string().nullable().compliance('none'),
    sections: fp.json<AnswerSectionDto[]>().compliance('none'),
    sentencesKept: fp.integer().compliance('none'),
    sentencesRemoved: fp.integer().compliance('none'),
    removedSentences: fp.json<RemovedSentenceRecord[]>().compliance('none'),
    durationMs: fp.integer().compliance('none')
  }
});

export type GeneratedAnswer = InferEntity<typeof GeneratedAnswer>;
