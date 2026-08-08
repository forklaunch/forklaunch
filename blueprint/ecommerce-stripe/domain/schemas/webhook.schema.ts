import { array, optional, string } from '../../schema';

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

/**
 * The PayPal webhook body shape (CHECKOUT.ORDER.APPROVED /
 * PAYMENT.CAPTURE.DENIED — the only two event types this app handles, see
 * webhook.controller.ts's switch — but shaped loosely enough, via the
 * `optional()`s below, to also validate the other event types that hit the
 * same route and fall into that switch's `default` case).
 *
 * Previously an inline `PaypalWebhookEvent` interface + `isPaypalWebhookEvent`
 * type guard duck-typed by hand inside webhook.controller.ts, with the route
 * itself declaring `body: unknown` — bypassing the schema pipeline every
 * other endpoint in this app goes through. Declaring it here instead means
 * malformed bodies 400 at the framework's own request-validation layer, the
 * same as any other endpoint, rather than via a hand-rolled guard.
 */
export const PaypalWebhookResourceSchema = {
  id: optional(string),
  supplementary_data: optional({
    related_ids: optional({
      order_id: optional(string)
    })
  }),
  purchase_units: optional(
    array({
      amount: optional({
        value: optional(string),
        currency_code: optional(string)
      })
    })
  )
};

export const PaypalWebhookEventSchema = {
  id: string,
  event_type: string,
  resource: PaypalWebhookResourceSchema
};

/**
 * The plain TS shape `PaypalWebhookEventSchema` validates at the route
 * boundary — kept as a hand-written parallel type rather than derived from
 * the runtime schema, same split the rest of this module uses between its
 * DTO types (interfaces-ecommerce/types) and its runtime schemas (this
 * file/checkout.schema.ts): e.g. ShippingAddressDto vs ShippingAddressSchema.
 */
export type PaypalWebhookEventDto = {
  id: string;
  event_type: string;
  resource: {
    id?: string;
    supplementary_data?: {
      related_ids?: {
        order_id?: string;
      };
    };
    purchase_units?: {
      amount?: { value?: string; currency_code?: string };
    }[];
  };
};

/**
 * Centralizes the nested-field access PayPal's two handlers in
 * webhook.controller.ts previously each did ad hoc:
 * `event.resource.id`, `event.resource.supplementary_data?.related_ids?.
 * order_id`, and `event.resource.purchase_units?.[0]?.amount` (both its
 * `value` and `currency_code`).
 */

/** CHECKOUT.ORDER.APPROVED's own resource id — the PayPal order id, stored as Payment.providerRef. */
export function getPaypalResourceId(
  event: PaypalWebhookEventDto
): string | undefined {
  return event.resource.id;
}

/**
 * PAYMENT.CAPTURE.DENIED's resource is the *capture*, not the order — its
 * own id is NOT the order id we stored as providerRef. PayPal links the two
 * via supplementary_data.related_ids.order_id.
 */
export function getPaypalRelatedOrderId(
  event: PaypalWebhookEventDto
): string | undefined {
  return event.resource.supplementary_data?.related_ids?.order_id;
}

/** Major-units decimal string (PayPal's amount format, e.g. "54.00") -> integer cents. */
function toCents(value: string): number {
  return Math.round(Number.parseFloat(value) * 100);
}

/** The first purchase unit's amount, in integer cents — undefined if absent/unparseable. */
export function getPaypalAmountCents(
  event: PaypalWebhookEventDto
): number | undefined {
  const value = event.resource.purchase_units?.[0]?.amount?.value;
  return value !== undefined ? toCents(value) : undefined;
}

/** The first purchase unit's currency, lower-cased to match this app's own currency convention. */
export function getPaypalCurrency(
  event: PaypalWebhookEventDto
): string | undefined {
  return event.resource.purchase_units?.[0]?.amount?.currency_code?.toLowerCase();
}
