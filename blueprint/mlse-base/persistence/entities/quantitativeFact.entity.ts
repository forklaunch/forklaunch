import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { Document } from './document.entity';
import { DocumentChunk } from './documentChunk.entity';
import { Topic } from './topic.entity';

/**
 * A number found in a cited passage (a dose, blood loss, duration, rate),
 * with its unit and the sentence it came from. Extracted values start as
 * 'unreviewed'; population and technique are filled in by a reviewer or the
 * verification step, and a number is never shown as verified before that.
 */
export const QuantitativeFact = defineComplianceEntity({
  name: 'QuantitativeFact',
  properties: {
    ...sqlBaseProperties,
    topic: () => fp.manyToOne(Topic),
    itemKey: fp.string().compliance('none'),
    chunk: () => fp.manyToOne(DocumentChunk),
    document: () => fp.manyToOne(Document),
    raw: fp.string().compliance('none'),
    low: fp.double().compliance('none'),
    high: fp.double().compliance('none'),
    unit: fp.string().compliance('none'),
    statistic: fp.string().nullable().compliance('none'),
    sentence: fp.string().compliance('none'),
    population: fp.string().nullable().compliance('none'),
    technique: fp.string().nullable().compliance('none'),
    // 'unreviewed' | 'verified' | 'rejected'
    reviewStatus: fp.string().compliance('none')
  }
});

export type QuantitativeFact = InferEntity<typeof QuantitativeFact>;
