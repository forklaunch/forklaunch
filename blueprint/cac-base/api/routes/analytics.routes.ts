import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import { getClaimAnalyticsSummary } from '../controllers/analytics.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const analyticsRouter = forklaunchRouter(
  '/analytics',
  schemaValidator,
  openTelemetryCollector
);

export const getClaimAnalyticsSummaryRoute = analyticsRouter.get(
  '/claims/summary',
  getClaimAnalyticsSummary
);
