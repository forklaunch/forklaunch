import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';

/**
 * One version of a document from a content source. A changed document gets a
 * new row with the next version and the previous row is marked superseded,
 * so history stays auditable while search only sees the current version.
 * Retracted documents keep their row but no passages, which removes them
 * from search immediately.
 *
 * Published literature only: no patient or organization data, so every field
 * is classified 'none'.
 */
export const Document = defineComplianceEntity({
  name: 'Document',
  properties: {
    ...sqlBaseProperties,
    sourceKey: fp.string().compliance('none'),
    externalId: fp.string().compliance('none'),
    version: fp.integer().compliance('none'),
    title: fp.string().compliance('none'),
    url: fp.string().compliance('none'),
    // ISO date; sources give year, year-month or a full date
    publishedAt: fp.string().nullable().compliance('none'),
    license: fp.string().nullable().compliance('none'),
    // 'full_text' | 'excerpt_only' | 'metadata_only', decided by the license gate
    licenseScope: fp.string().compliance('none'),
    isCaseReport: fp.boolean().compliance('none'),
    // see domain/enum/documentStatus.enum.ts; kept out of this module because
    // every export of persistence/entities is registered as an entity
    status: fp.string().compliance('none'),
    contentHash: fp.string().compliance('none'),
    meshDescriptorUis: fp.string().array().compliance('none'),
    supersededAt: fp.datetime().nullable().compliance('none')
  }
});

export type Document = InferEntity<typeof Document>;
