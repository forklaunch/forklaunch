import { forklaunchRouter, schemaValidator } from '../../schema';
import { ci, tokens } from '../../bootstrapper';
import {
  handlePaypalWebhook,
  handleStripeWebhook
} from '../controllers/webhook.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

// The raw-body hook MUST be configured here, on this router, not on the app.
// Router's constructor builds its own contentParse(options) from the options
// *it* was given (framework/express/src/expressRouter.ts:76), and mounting a
// router with .use() does not re-parent it to the app's parsers — so an
// app-level json.verify never reaches these routes and req.rawBody would be
// undefined on every delivery, 400ing every legitimate Stripe webhook.
//
// Declaring `body: { text: string }` instead (as billing-stripe does) does not
// work either: contentParse builds express.text(options?.text), which defaults
// to type 'text/plain', and Stripe sends application/json — so the body is
// never parsed. billing-stripe has this bug today; see the follow-up ticket.
export const webhookRouter = forklaunchRouter(
  '/webhook',
  schemaValidator,
  openTelemetryCollector,
  {
    json: {
      verify: (req, _res, buf) => {
        (req as { rawBody?: Buffer }).rawBody = Buffer.from(buf);
      }
    }
  }
);

export const handleStripeWebhookRoute = webhookRouter.post(
  '/stripe',
  handleStripeWebhook
);
export const handlePaypalWebhookRoute = webhookRouter.post(
  '/paypal',
  handlePaypalWebhook
);
