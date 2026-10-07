import { optional, string } from '../../schema';
import { OrderSchemas, PaymentSchemas } from '.';

/**
 * Request/response schemas for the checkout endpoint.
 *
 * Live here rather than inline in the controller so the controller stays
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

/**
 * Checkout's response: the order it created, plus the payment it created
 * for that order's total.
 *
 * `clientSecret` is Stripe-only: Stripe.js needs the PaymentIntent's
 * client_secret client-side to actually collect the charge — the
 * PaymentIntent id alone (exposed as payment.providerRef) isn't enough for
 * that handshake. PayPal has no equivalent secret: payment.providerRef *is*
 * the PayPal order id, and that's everything the PayPal JS SDK needs to
 * render its approval buttons, so clientSecret is simply absent on that
 * path — hence optional here rather than a second, provider-specific
 * response field.
 *
 * clientSecret is deliberately never part of PaymentSchemas.PaymentSchema
 * itself (see Payment's entity definition — there's no clientSecret
 * column): it's a one-time, provider-issued credential relevant only to
 * *this* response, not something a later `GET /payment/:id` should ever be
 * able to replay.
 */
export const CheckoutResultSchema = {
  order: OrderSchemas.OrderSchema,
  payment: PaymentSchemas.PaymentSchema,
  clientSecret: optional(string)
};
