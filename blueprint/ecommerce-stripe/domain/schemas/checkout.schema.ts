import { optional, string } from '../../schema';

/**
 * Request schema for the checkout endpoint.
 *
 * Lives here rather than inline in the controller so the controller stays
 * HTTP concerns only, matching how billing/iam keep schema definitions out
 * of their controllers (see catalogImport.schema.ts for the same pattern).
 */
export const ShippingAddressSchema = {
  name: string,
  line1: string,
  line2: optional(string),
  city: string,
  state: string,
  postalCode: string,
  country: string
};
