import { forklaunchRouter, schemaValidator } from '../../schema';
import { ci, tokens } from '../../bootstrapper';
import {
  handlePaypalWebhook,
  handleStripeWebhook
} from '../controllers/webhook.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

// Both handlers verify a provider signature over the exact request bytes.
// The framework's body parser keeps them on `req._rawBody` for Express and
// hyper-express, so this router needs no parser options of its own (an
// Express-only `json.verify` hook here did not compile for hyper-express).
export const webhookRouter = forklaunchRouter(
  '/webhook',
  schemaValidator,
  openTelemetryCollector
);

export const handleStripeWebhookRoute = webhookRouter.post(
  '/stripe',
  handleStripeWebhook
);
export const handlePaypalWebhookRoute = webhookRouter.post(
  '/paypal',
  handlePaypalWebhook
);
