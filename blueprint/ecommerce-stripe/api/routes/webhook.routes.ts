import { forklaunchRouter, schemaValidator } from '../../schema';
import { ci, tokens } from '../../bootstrapper';
import {
  handlePaypalWebhook,
  handleStripeWebhook
} from '../controllers/webhook.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

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
