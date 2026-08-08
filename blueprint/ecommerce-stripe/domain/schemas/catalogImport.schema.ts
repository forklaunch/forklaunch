import {
  array,
  boolean,
  number,
  optional,
  record,
  string
} from '../../schema';

/**
 * Request schemas for the bulk catalog-import endpoint.
 *
 * These live here rather than inline in the controller so the controller
 * stays HTTP concerns only, matching how billing/iam keep schema definitions
 * out of their controllers.
 */
export const ImportOptionSchema = {
  name: string,
  isPackQuantity: boolean,
  values: array(string)
};

export const ImportImageSchema = {
  src: string,
  position: number
};

export const ImportVariantSchema = {
  externalId: string,
  sku: optional(string),
  title: string,
  optionValues: optional(record(string, string)),
  priceCents: number,
  compareAtPriceCents: optional(number),
  requiresShipping: optional(boolean),
  /** Seed stock — no reservation semantics in v1 (ECOM-04). */
  initialStock: optional(number)
};

export const ImportProductSchema = {
  externalId: string,
  handle: string,
  sourceUrl: optional(string),
  title: string,
  descriptionHtml: optional(string),
  vendor: optional(string),
  productType: optional(string),
  tags: optional(array(string)),
  options: optional(array(ImportOptionSchema)),
  images: optional(array(ImportImageSchema)),
  variants: array(ImportVariantSchema)
};
