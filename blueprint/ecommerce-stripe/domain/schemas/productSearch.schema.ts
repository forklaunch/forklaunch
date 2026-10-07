import { array, boolean, number, optional, string } from '../../schema';

/**
 * Query schema for the product list/search endpoint.
 *
 * Lives here rather than inline in the controller so the controller stays
 * HTTP concerns only, matching catalogImport.schema.ts and checkout.schema.ts.
 */
export const ProductSearchQuerySchema = {
  ids: optional(array(string)),
  title: optional(string),
  minPriceCents: optional(number),
  maxPriceCents: optional(number),
  inStock: optional(boolean),
  optionName: optional(string),
  optionValue: optional(string)
};
