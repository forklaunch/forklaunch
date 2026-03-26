import { handlers, schemaValidator, string } from '@forklaunch/blueprint-core';
import { default as Stripe, default as stripe } from 'stripe';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OpenTelemetryCollector);
const serviceFactory = ci.scopedResolver(tokens.WebhookService);
const STRIPE_WEBHOOK_SECRET = ci.resolve(tokens.STRIPE_WEBHOOK_SECRET);

export const handleWebhookEvent = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'handleWebhookEvent',
    access: 'public',
    summary: 'Handle a stripe event via webhook',
    body: {
      text: string
    },
    requestHeaders: {
      'stripe-signature': string
    },
    responses: {
      200: string
    }
  },
  async (req, res) => {
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
