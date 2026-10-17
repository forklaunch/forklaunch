import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { VectorType } from '../types/vector.type';
import { Document } from './document.entity';

/**
 * A searchable passage of a document, the unit MLSE retrieves and cites. The
 * table also carries a generated full-text column (search_vector) maintained
 * by PostgreSQL itself, which is why it is not mapped here.
 *
 * Deliberately no organizationId yet: the shared corpus is not tenant data,
 * and a nullable organizationId would engage the framework's tenant filter,
 * hiding shared passages from every tenant-scoped query. Licensed,
 * organization-owned passages get their own handling with licensed content.
 */
export const DocumentChunk = defineComplianceEntity({
  name: 'DocumentChunk',
  properties: {
    ...sqlBaseProperties,
    document: () => fp.manyToOne(Document),
    sectionPath: fp.string().compliance('none'),
    ordinal: fp.integer().compliance('none'),
    text: fp.string().compliance('none'),
    embedding: fp.type(VectorType).nullable().compliance('none'),
    embeddingModel: fp.string().nullable().compliance('none')
  }
});

export type DocumentChunk = InferEntity<typeof DocumentChunk>;
