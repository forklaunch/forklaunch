import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  createPaymentLink,
  expirePaymentLink,
  getPaymentLink,
  handlePaymentFailure,
  handlePaymentSuccess,
  listPaymentLinks,
  updatePaymentLink
} from '../controllers/paymentLink.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const paymentLinkRouter = forklaunchRouter(
  '/payment-link',
  schemaValidator,
  openTelemetryCollector
);

export const createPaymentLinkRoute = paymentLinkRouter.post(
  '/',
  createPaymentLink
);
export const listPaymentLinksRoute = paymentLinkRouter.get(
  '/',
  listPaymentLinks
);
export const handlePaymentSuccessRoute = paymentLinkRouter.get(
  '/:id/success',
  handlePaymentSuccess
);
export const handlePaymentFailureRoute = paymentLinkRouter.get(
  '/:id/failure',
  handlePaymentFailure
);
export const getPaymentLinkRoute = paymentLinkRouter.get(
  '/:id',
  getPaymentLink
);
export const updatePaymentLinkRoute = paymentLinkRouter.put(
  '/:id',
  updatePaymentLink
);
export const expirePaymentLinkRoute = paymentLinkRouter.delete(
  '/:id',
  expirePaymentLink
);
