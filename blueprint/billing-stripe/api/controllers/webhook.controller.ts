import { handlers, schemaValidator, string } from '@forklaunch/blueprint-core';
import { default as Stripe, default as stripe } from 'stripe';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const serviceFactory = ci.scopedResolver(tokens.WebhookService);
const STRIPE_WEBHOOK_SECRET = ci.resolve(tokens.STRIPE_WEBHOOK_SECRET);

export const handleWebhookEvent = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'handleWebhookEvent',
    access: 'public',
    summary: 'Handle a stripe event via webhook',
    // Stripe posts application/json and signs the exact bytes: declare the
    // body as text with a json contentType so req.body is the raw payload
    // string that stripe.webhooks.constructEvent verifies against.
    body: {
      text: string,
      contentType: 'application/json'
    },
    requestHeaders: {
      'stripe-signature': string
    },
    responses: {
      200: string,
      400: string
    }
  },
  async (req, res) => {
    // A managed instance holds no webhook secret: Stripe's events for its
    // connected account arrive from the platform at /platform-events/payments,
    // verified with verifyPlatformEvent (api/platformEvents/payments.ts).
    if (!STRIPE_WEBHOOK_SECRET) {
      return res
        .status(400)
        .send(
          'Stripe webhooks are not received here without STRIPE_WEBHOOK_SECRET; a managed instance receives them as platform events'
        );
    }
    const signature = req.headers['stripe-signature'];
    let event: Stripe.Event;
    try {
      event = stripe.webhooks.constructEvent(
        req.body,
        signature,
        STRIPE_WEBHOOK_SECRET
      );
    } catch (err) {
      openTelemetryCollector.error(
        `Webhook signature verification failed.`,
        err instanceof Error ? err.message : 'Unknown error'
      );
      return res.status(400).send('Webhook signature verification failed');
    }
    openTelemetryCollector.debug('Processing stripe event', event);
    await serviceFactory().handleWebhookEvent(event);
    res.status(200).send('Webhook event processed');
  }
);
