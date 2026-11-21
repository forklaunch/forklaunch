import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { Document } from './document.entity';
import { DocumentChunk } from './documentChunk.entity';
import { Topic } from './topic.entity';

/**
 * Links a question or phase of a topic's framework to a passage that
 * addresses it: the evidence map answers are later written from. Deleting a
 * passage (for example when its document is retracted) removes the link.
 */
export const TopicEvidence = defineComplianceEntity({
  name: 'TopicEvidence',
  properties: {
    ...sqlBaseProperties,
    topic: () => fp.manyToOne(Topic),
    // framework item key, e.g. 'risks' or 'blood'
    itemKey: fp.string().compliance('none'),
    // 'question' | 'phase'
    itemKind: fp.string().compliance('none'),
    chunk: () => fp.manyToOne(DocumentChunk),
    document: () => fp.manyToOne(Document),
    rank: fp.integer().compliance('none'),
    score: fp.double().compliance('none')
  }
});

export type TopicEvidence = InferEntity<typeof TopicEvidence>;
