import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import { buildClaim, scrubClaim } from '../controllers/claim.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const claimRouter = forklaunchRouter(
  '/claim',
  schemaValidator,
  openTelemetryCollector
);

export const buildClaimRoute = claimRouter.post('/build', buildClaim);
export const scrubClaimRoute = claimRouter.post('/:id/scrub', scrubClaim);
