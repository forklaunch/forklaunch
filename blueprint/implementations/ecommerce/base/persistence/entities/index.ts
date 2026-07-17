import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';

/**
 * Base template entities — the typing scaffolds the generic base services
 * operate against. The deployable app defines the concrete entities (with
 * sqlBaseProperties + relations) and injects them via mappers.
 *
 * Catalog data (products, variants, stock) carries no PII/PCI, so every field
 * is compliance('none'). PII/PCI tagging becomes relevant with customer/order
 * entities in later stages.
 *
 * Added incrementally as each entity's PR lands.
 */
export const Variant = defineComplianceEntity({
  name: 'Variant',
  properties: {
    id: fp.string().primary().compliance('none'),
    productId: fp.string().compliance('none'),
    externalId: fp.string().compliance('none'),
    sku: fp.string().nullable().compliance('none'),
    title: fp.string().compliance('none'),
    optionValues: fp.json<Record<string, string>>().nullable().compliance('none'),
    priceCents: fp.integer().compliance('none'),
    compareAtPriceCents: fp.integer().nullable().compliance('none'),
    requiresShipping: fp.boolean().compliance('none')
  }
});
