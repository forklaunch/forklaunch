import { randomUUID } from 'node:crypto';
import { handlers, schemaValidator, string, unknown } from '../../schema';
import { OrderStatus } from '@forklaunch/interfaces-ecommerce/types';
import { default as Stripe, default as stripe } from 'stripe';
import {
  getPaypalAmountCents,
  getPaypalCurrency,
  getPaypalRelatedOrderId,
  getPaypalResourceId,
  PaypalWebhookEventDto,
  PaypalWebhookEventSchema,
  PaypalWebhookHeadersSchema,
  StripeWebhookHeadersSchema
} from '../../domain/schemas/webhook.schema';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const stripePaymentServiceFactory = ci.scopedResolver(tokens.PaymentService);
const paypalPaymentServiceFactory = ci.scopedResolver(
  tokens.PaypalPaymentService
);
const paypalClient = ci.resolve(tokens.PaypalClient);
const orderServiceFactory = ci.scopedResolver(tokens.OrderService);
const orderEventProducerFactory = ci.scopedResolver(tokens.OrderEventProducer);
const webhookEventServiceFactory = ci.scopedResolver(
  tokens.WebhookEventService
);
const STRIPE_WEBHOOK_SECRET = ci.resolve(tokens.STRIPE_WEBHOOK_SECRET);
const PAYPAL_WEBHOOK_ID = ci.resolve(tokens.PAYPAL_WEBHOOK_ID);

/**
 * Moves an order from PENDING to PAID and enqueues the same OrderEventRecord
 * order.controller.ts's own `transitionOrder` HTTP handler enqueues on every
 * legal transition — worker.ts's inventory decrement is driven by that
 * queue, not by the transition call itself, so skipping this enqueue would
 * silently mean inventory never decrements for a webhook-driven PAID
 * transition. Idempotent by construction: only acts if the order is still
 * PENDING, so a redelivered/duplicate "succeeded" event (or one arriving
 * after some other path already moved the order) is a safe no-op.
 */
async function transitionOrderToPaid(orderId: string): Promise<void> {
  const before = await orderServiceFactory().getOrder({ id: orderId });
  if (before.status !== OrderStatus.PENDING) {
    openTelemetryCollector.info(
      'Order already past pending — payment-succeeded webhook is a no-op for order state',
      { orderId, status: before.status }
    );
    return;
  }

  const updated = await orderServiceFactory().transitionOrder({
    id: orderId,
    to: OrderStatus.PAID
  });

  const now = new Date();
  await orderEventProducerFactory().enqueueJob({
    id: randomUUID(),
    orderId: updated.id,
    fromStatus: before.status,
    toStatus: updated.status,
    items: updated.items,
    processed: false,
    retryCount: 0,
    retentionAnonymizedAt: null,
    createdAt: now,
    updatedAt: now
  });
}

async function handleStripePaymentSucceeded(
  paymentIntent: Stripe.PaymentIntent
): Promise<void> {
  // confirmPayment is idempotent (base service no-ops if already
  // SUCCEEDED) and looks the payment up by providerRef — it never trusts
  // anything else from the webhook payload.
  const payment = await stripePaymentServiceFactory().confirmPayment({
    providerRef: paymentIntent.id
  });

  // Never trust the webhook's amount/currency for anything money-moving —
  // reconcile against what we actually stored when the PaymentIntent was
  // created. A mismatch here can only mean an internal bookkeeping bug
  // (paymentIntent.id is Stripe's own id, already signature-verified, so
  // this isn't a spoofing vector) — but "probably a bug" still isn't
  // "safe to decrement inventory and mark the order paid", so the order
  // transition (and the inventory decrement it drives) is gated on this
  // check, even though the Payment row itself is already flipped to
  // SUCCEEDED above.
  if (
    payment.amountCents !== paymentIntent.amount ||
    payment.currency !== paymentIntent.currency
  ) {
    openTelemetryCollector.error(
      'Stripe webhook amount/currency mismatch — payment record does not match provider event; order NOT transitioned',
      {
        paymentId: payment.id,
        orderId: payment.orderId,
        storedAmountCents: payment.amountCents,
        storedCurrency: payment.currency,
        eventAmountCents: paymentIntent.amount,
        eventCurrency: paymentIntent.currency
      }
    );
    return;
  }

  await transitionOrderToPaid(payment.orderId);
}

async function handleStripePaymentFailed(
  paymentIntent: Stripe.PaymentIntent
): Promise<void> {
  const payment = await stripePaymentServiceFactory().failPayment({
    providerRef: paymentIntent.id
  });
  // Deliberately NOT auto-cancelling the order — OrderStatus has no
  // "payment failed" state distinct from PENDING, and a declined
  // card/insufficient funds is routinely retried by the customer against
  // the same order. See checkout.controller.ts's payment try/catch for the
  // matching decision on the checkout-time failure path.
  openTelemetryCollector.warn(
    'Stripe payment failed — order left pending for a possible retry',
    { paymentId: payment.id, orderId: payment.orderId }
  );
}

export const handleStripeWebhook = handlers.post(
  schemaValidator,
  '/stripe',
  {
    name: 'Handle Stripe Webhook',
    // Not this repo's own HMAC scheme — Stripe is the caller here, not an
    // internal service holding our HMAC secret. Authenticity is instead
    // verified by hand below via stripe.webhooks.constructEvent.
    access: 'public',
    summary: "Handle a Stripe payment webhook event (Stripe's own signature verification, not this app's HMAC)",
    body: unknown,
    requestHeaders: StripeWebhookHeadersSchema,
    responses: {
      200: string,
      400: string
    }
  },
  async (req, res) => {
    // Captured by the `verify` hook on the app's json body-parser (see
    // server.ts) — Stripe's signature is computed over the exact raw bytes
    // it sent, so the already-JSON-parsed req.body (property order/
    // whitespace not guaranteed identical) cannot be used here.
    const rawBody = (req as unknown as { rawBody?: Buffer }).rawBody;
    const signature = req.headers['stripe-signature'];

    if (!rawBody || !signature) {
      res.status(400).send('Missing request body or stripe-signature header');
      return;
    }

    let event: Stripe.Event;
    try {
      event = stripe.webhooks.constructEvent(
        rawBody,
        signature,
        STRIPE_WEBHOOK_SECRET
      );
    } catch (error) {
      // Never fall through to processing on a bad/missing signature.
      openTelemetryCollector.error(
        'Stripe webhook signature verification failed',
        error instanceof Error ? error.message : 'Unknown error'
      );
      res.status(400).send('Webhook signature verification failed');
      return;
    }

    const outcome = await webhookEventServiceFactory().beginProcessing({
      provider: 'stripe',
      providerEventId: event.id,
      eventType: event.type
    });
    if (outcome === 'already-processed') {
      res.status(200).send('Webhook event already processed');
      return;
    }

    try {
      switch (event.type) {
        case 'payment_intent.succeeded':
          // Stripe guarantees data.object is a PaymentIntent for this event
          // type; cast explicitly rather than relying on `Stripe.Event`
          // narrowing via `event.type`, which this SDK's types don't
          // reliably do.
          await handleStripePaymentSucceeded(
            event.data.object as Stripe.PaymentIntent
          );
          break;
        case 'payment_intent.payment_failed':
          await handleStripePaymentFailed(
            event.data.object as Stripe.PaymentIntent
          );
          break;
        default:
          openTelemetryCollector.info(
            'Unprocessed Stripe webhook event type',
            event.type
          );
      }
    } catch (error) {
      // Leave the WebhookEvent row unprocessed — Stripe retries on a
      // non-2xx response, and the next delivery will retry this same
      // handling (confirmPayment/failPayment/transitionOrder are all
      // themselves idempotent, so re-running is safe).
      openTelemetryCollector.error('Stripe webhook handling failed', error);
      res.status(400).send('Webhook handling failed');
      return;
    }

    await webhookEventServiceFactory().markProcessed({
      provider: 'stripe',
      providerEventId: event.id
    });
    res.status(200).send('Webhook event processed');
  }
);

async function handlePaypalOrderApproved(
  event: PaypalWebhookEventDto
): Promise<void> {
  const providerRef = getPaypalResourceId(event);
  if (!providerRef) {
    openTelemetryCollector.error(
      'PayPal CHECKOUT.ORDER.APPROVED event missing resource.id — cannot resolve payment',
      { eventId: event.id }
    );
    return;
  }

  // confirmPayment on the PayPal service actually captures the order
  // (real funds movement) before flipping the stored Payment to SUCCEEDED
  // — see PaypalPaymentService.confirmPayment. That's intentional and
  // correct here, not a shortcut: the signature check above already
  // proved this event genuinely came from PayPal for an order the buyer
  // approved, and PayPal's capture call charges whatever amount *PayPal
  // itself* has on file for that order id — not anything from this
  // webhook's JSON body — so there's no amount-spoofing vector in the
  // capture step itself.
  const payment = await paypalPaymentServiceFactory().confirmPayment({
    providerRef
  });

  // As with Stripe: reconcile our own bookkeeping against the event before
  // trusting it enough to transition the order (and drive the inventory
  // decrement that comes with it). Purely a drift/bug detector, same
  // reasoning as handleStripePaymentSucceeded above.
  const eventAmountCents = getPaypalAmountCents(event);
  const eventCurrency = getPaypalCurrency(event);
  if (
    eventAmountCents === undefined ||
    eventCurrency === undefined ||
    payment.amountCents !== eventAmountCents ||
    payment.currency !== eventCurrency
  ) {
    openTelemetryCollector.error(
      'PayPal webhook amount/currency mismatch or missing — payment record does not match provider event; order NOT transitioned',
      {
        paymentId: payment.id,
        orderId: payment.orderId,
        storedAmountCents: payment.amountCents,
        storedCurrency: payment.currency,
        eventAmountCents,
        eventCurrency
      }
    );
    return;
  }

  await transitionOrderToPaid(payment.orderId);
}

async function handlePaypalCaptureDenied(
  event: PaypalWebhookEventDto
): Promise<void> {
  // A capture's own id is NOT the order id we stored as providerRef —
  // PayPal links the two via supplementary_data.related_ids.order_id.
  const providerRef = getPaypalRelatedOrderId(event);
  if (!providerRef) {
    openTelemetryCollector.error(
      'PayPal PAYMENT.CAPTURE.DENIED event missing resource.supplementary_data.related_ids.order_id — cannot resolve payment',
      { eventId: event.id }
    );
    return;
  }

  const payment = await paypalPaymentServiceFactory().failPayment({
    providerRef
  });
  // Same v1 decision as the Stripe failure path — see
  // handleStripePaymentFailed for the reasoning.
  openTelemetryCollector.warn(
    'PayPal payment capture denied — order left pending for a possible retry',
    { paymentId: payment.id, orderId: payment.orderId }
  );
}

export const handlePaypalWebhook = handlers.post(
  schemaValidator,
  '/paypal',
  {
    name: 'Handle PayPal Webhook',
    // See handleStripeWebhook — same reasoning, PayPal's own transmission
    // signature is verified by hand below instead.
    access: 'public',
    summary: "Handle a PayPal order-approval/capture-denial webhook event (PayPal's own signature verification, not this app's HMAC)",
    // Runs through the schema pipeline like every other endpoint now —
    // previously `unknown`, with shape validation duck-typed by hand via an
    // inline type guard (see webhook.schema.ts's PaypalWebhookEventSchema
    // doc comment for why that moved).
    body: PaypalWebhookEventSchema,
    requestHeaders: PaypalWebhookHeadersSchema,
    responses: {
      200: string,
      400: string
    }
  },
  async (req, res) => {
    const event = req.body;

    // PayPal verifies the transmission signature against its own record of
    // what it sent (matched by transmission id) — unlike Stripe, this does
    // not require reproducing the exact raw bytes locally, so the already-
    // parsed body is fine to send back as `webhook_event`.
    const verified = await paypalClient.verifyWebhookSignature({
      transmissionId: req.headers['paypal-transmission-id'],
      transmissionTime: req.headers['paypal-transmission-time'],
      transmissionSig: req.headers['paypal-transmission-sig'],
      certUrl: req.headers['paypal-cert-url'],
      authAlgo: req.headers['paypal-auth-algo'],
      webhookId: PAYPAL_WEBHOOK_ID,
      webhookEvent: event
    });
    if (!verified) {
      // Never fall through to processing on a failed/unverifiable
      // signature — this includes PayPal API errors (verifyWebhookSignature
      // fails closed on those too, see paypal-client.ts).
      openTelemetryCollector.error(
        'PayPal webhook signature verification failed',
        { eventId: event.id }
      );
      res.status(400).send('Webhook signature verification failed');
      return;
    }

    const outcome = await webhookEventServiceFactory().beginProcessing({
      provider: 'paypal',
      providerEventId: event.id,
      eventType: event.event_type
    });
    if (outcome === 'already-processed') {
      res.status(200).send('Webhook event already processed');
      return;
    }

    try {
      switch (event.event_type) {
        case 'CHECKOUT.ORDER.APPROVED':
          await handlePaypalOrderApproved(event);
          break;
        case 'PAYMENT.CAPTURE.DENIED':
          await handlePaypalCaptureDenied(event);
          break;
        default:
          openTelemetryCollector.info(
            'Unprocessed PayPal webhook event type',
            event.event_type
          );
      }
    } catch (error) {
      // Same reasoning as the Stripe handler: leave the row unprocessed so
      // a PayPal retry re-runs handling, which is safe because
      // confirmPayment/failPayment/transitionOrder are all idempotent.
      openTelemetryCollector.error('PayPal webhook handling failed', error);
      res.status(400).send('Webhook handling failed');
      return;
    }

    await webhookEventServiceFactory().markProcessed({
      provider: 'paypal',
      providerEventId: event.id
    });
    res.status(200).send('Webhook event processed');
  }
);
