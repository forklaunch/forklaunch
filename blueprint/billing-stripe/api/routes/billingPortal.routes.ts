import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  createBillingPortalSession,
  expireBillingPortalSession,
  getBillingPortalSession,
  updateBillingPortalSession
} from '../controllers/billingPortal.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const billingPortalRouter = forklaunchRouter(
  '/billing-portal',
  schemaValidator,
  openTelemetryCollector
);

export const createBillingPortalRoute = billingPortalRouter.post(
  '/',
  createBillingPortalSession
);
export const getBillingPortalRoute = billingPortalRouter.get(
  '/:id',
  getBillingPortalSession
);
export const updateBillingPortalRoute = billingPortalRouter.put(
  '/:id',
  updateBillingPortalSession
);
export const expireBillingPortalRoute = billingPortalRouter.delete(
  '/:id',
  expireBillingPortalSession
);
