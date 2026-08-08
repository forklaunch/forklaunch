import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';

export const Variant = defineComplianceEntity({
  name: 'Variant',
  properties: {
    ...sqlBaseProperties,
    productId: fp.string().compliance('none'),
    externalId: fp.string().unique().compliance('none'),
    // Nullable — many real catalogs never set a SKU (see migration findings);
    // externalId is the reliable dedupe/lookup key, not sku.
    sku: fp.string().nullable().compliance('none'),
    title: fp.string().compliance('none'),
    optionValues: fp.json<Record<string, string>>().nullable().compliance('none'),
    priceCents: fp.integer().compliance('none'),
    compareAtPriceCents: fp.integer().nullable().compliance('none'),
    requiresShipping: fp.boolean().compliance('none')
  }
});
