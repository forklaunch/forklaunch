import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  cancelSubscription,
  createSubscription,
  deleteSubscription,
  getOrganizationSubscription,
  getSubscription,
  getUserSubscription,
  listSubscriptions,
  resumeSubscription,
  updateSubscription
} from '../controllers/subscription.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const subscriptionRouter = forklaunchRouter(
  '/subscription',
  schemaValidator,
  openTelemetryCollector
);

export const createSubscriptionRoute = subscriptionRouter.post(
  '/',
  createSubscription
);
export const listSubscriptionsRoute = subscriptionRouter.get(
  '/',
  listSubscriptions
);
export const getUserSubscriptionRoute = subscriptionRouter.get(
  '/user/:id',
  getUserSubscription
);
export const getOrganizationSubscriptionRoute = subscriptionRouter.get(
  '/organization/:id',
  getOrganizationSubscription
);
export const cancelSubscriptionRoute = subscriptionRouter.get(
  '/:id/cancel',
  cancelSubscription
);
export const resumeSubscriptionRoute = subscriptionRouter.get(
  '/:id/resume',
  resumeSubscription
);
export const getSubscriptionRoute = subscriptionRouter.get(
  '/:id',
  getSubscription
);
export const updateSubscriptionRoute = subscriptionRouter.put(
  '/:id',
  updateSubscription
);
export const deleteSubscriptionRoute = subscriptionRouter.delete(
  '/:id',
  deleteSubscription
);
