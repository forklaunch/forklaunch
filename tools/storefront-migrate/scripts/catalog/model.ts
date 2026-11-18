/**
 * The normalized catalog model — the shape we reshape any source (Shopify
 * products.json, later WooCommerce/etc.) INTO before importing into the
 * ForkLaunch ecommerce module. Deliberately source-agnostic and close to the
 * module's own Product -> Variant -> Inventory data model.
 *
 * Money is carried as integer cents (never floats) to match ForkLaunch's
 * money convention (see the Guild deck: "Money handled as whole cents").
 */

export interface NormalizedCatalog {
  source: {
    shop: string;
    sourceUrl: string;
    platform: 'shopify';
    pulledAt: string;
    rawProductCount: number;
    keptProductCount: number;
    filteredProductCount: number;
  };
  products: NormalizedProduct[];
}

export interface NormalizedProduct {
  /** Stable id from the source platform, kept so re-imports can dedupe. */
  externalId: string;
  /** URL slug — preserved so SEO redirects can map old links later. */
  handle: string;
  /** Full original product URL, reconstructed from shop + handle. */
  sourceUrl: string;
  title: string;
  descriptionHtml: string;
  vendor: string;
  productType: string;
  tags: string[];
  /** Option dimensions (0, 1, or 2+), names cleaned. Never hardcoded. */
  options: NormalizedOption[];
  images: NormalizedImage[];
  variants: NormalizedVariant[];
}

export interface NormalizedOption {
  name: string;
  /** Set true when a "Size"-named option actually encodes pack quantity. */
  isPackQuantity: boolean;
  values: string[];
}

export interface NormalizedImage {
  src: string;
  position: number;
}

export interface NormalizedVariant {
  externalId: string;
  sku: string;
  title: string;
  /** Cleaned option name -> value, e.g. { "Color": "Black" }. */
  optionValues: Record<string, string>;
  priceCents: number;
  /** Original price when on sale (compare-at), in cents. */
  compareAtPriceCents: number | null;
  available: boolean;
  /**
   * Real on-hand stock count. Only knowable from the credentialed Admin API —
   * the public products.json feed exposes `available` (a boolean) but never a
   * count, so a public pull leaves this null and the importer falls back to a
   * placeholder. An Admin pull fills it with the true quantity, which is what
   * makes the migrated inventory exact rather than seeded.
   */
  inventoryQuantity: number | null;
  requiresShipping: boolean;
  grams: number;
}

/** A single thing the importer/model could not fully handle, for the gap report. */
export interface GapNote {
  kind: string;
  detail: string;
  productHandle?: string;
}
