import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';

/**
 * The registry of content sources MLSE draws from, one row per source.
 *
 * Literature is shared across organizations, not tenant data, so there is no
 * organizationId here and every field is classified 'none'. Ingestion records
 * when each source was last refreshed so a source falling behind its cadence
 * can be reported rather than silently going stale.
 */
export const Source = defineComplianceEntity({
  name: 'Source',
  properties: {
    ...sqlBaseProperties,
    // the provider's stable id, e.g. 'openfda' or 'pmc_oa'
    sourceKey: fp.string().unique().compliance('none'),
    name: fp.string().compliance('none'),
    tier: fp.string().compliance('none'),
    licenseTerms: fp.string().compliance('none'),
    commercialUse: fp.boolean().compliance('none'),
    liveQuery: fp.boolean().compliance('none'),
    // licensed (contract) content: searchable only for organizations with an
    // active ContentLicense, and not ingested while none is active
    requiresLicense: fp.boolean().default(false).compliance('none'),
    lastRefreshedAt: fp.datetime().nullable().compliance('none')
  }
});

export type Source = InferEntity<typeof Source>;
