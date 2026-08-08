import { string } from '../../schema';

/**
 * Request header schemas for the two webhook endpoints.
 *
 * Live here rather than inline in the controller so the controller stays
 * HTTP concerns only, matching how billing/iam keep schema definitions out
 * of their controllers (see catalogImport.schema.ts for the same pattern).
 *
 * Both webhook routes declare `access: 'public'` (see webhook.controller.ts)
 * — these headers are NOT this repo's own HMAC scheme (that's the
 * `auth: { hmac: ... }` contract every other internal endpoint in this app
 * uses). They're each provider's own signature headers, verified by hand
 * inside the handler against that provider's SDK/API, because the caller
 * here is Stripe/PayPal, not another internal service holding our shared
 * HMAC secret.
 */
export const StripeWebhookHeadersSchema = {
  'stripe-signature': string
};

/**
 * PayPal's five verification headers (case-insensitive on the wire; Express
 * lower-cases incoming header names, hence the lowercase keys here).
 */
export const PaypalWebhookHeadersSchema = {
  'paypal-transmission-id': string,
  'paypal-transmission-time': string,
  'paypal-transmission-sig': string,
  'paypal-cert-url': string,
  'paypal-auth-algo': string
};
