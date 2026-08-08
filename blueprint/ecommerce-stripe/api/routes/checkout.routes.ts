import { forklaunchRouter, schemaValidator } from '../../schema';
import { ci, tokens } from '../../bootstrapper';
import { checkout } from '../controllers/checkout.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const checkoutRouter = forklaunchRouter(
  '/checkout',
  schemaValidator,
  openTelemetryCollector
);

export const checkoutRoute = checkoutRouter.post('/', checkout);
