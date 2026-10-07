import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  createCheckoutSession,
  expireCheckoutSession,
  getCheckoutSession,
  handleCheckoutFailure,
  handleCheckoutSuccess
} from '../controllers/checkoutSession.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const checkoutSessionRouter = forklaunchRouter(
  '/checkout-session',
  schemaValidator,
  openTelemetryCollector
);

export const createCheckoutSessionRoute = checkoutSessionRouter.post(
  '/',
  createCheckoutSession
);
export const getCheckoutSessionRoute = checkoutSessionRouter.get(
  '/:id',
  getCheckoutSession
);
export const expireCheckoutSessionRoute = checkoutSessionRouter.get(
  '/:id/expire',
  expireCheckoutSession
);
export const handleCheckoutSuccessRoute = checkoutSessionRouter.get(
  '/:id/success',
  handleCheckoutSuccess
);
export const handleCheckoutFailureRoute = checkoutSessionRouter.get(
  '/:id/failure',
  handleCheckoutFailure
);
