import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';

/**
 * An organization's license to a licensed source. Its content is searchable
 * for that organization only while the license is active and within its
 * dates; a revoked or expired license hides it again at once.
 */
export const ContentLicense = defineComplianceEntity({
  name: 'ContentLicense',
  properties: {
    ...sqlBaseProperties,
    organizationId: fp.string().compliance('none'),
    sourceKey: fp.string().compliance('none'),
    // who holds the contract, e.g. the hospital group
    licensee: fp.string().compliance('none'),
    // contract or order reference
    reference: fp.string().nullable().compliance('none'),
    // 'active' | 'revoked'
    status: fp.string().compliance('none'),
    validFrom: fp.datetime().compliance('none'),
    validUntil: fp.datetime().nullable().compliance('none'),
    createdBy: fp.string().compliance('none')
  }
});

export type ContentLicense = InferEntity<typeof ContentLicense>;
