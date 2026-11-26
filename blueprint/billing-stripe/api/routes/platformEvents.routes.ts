import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import { receivePlatformEvent } from '../controllers/platformEvents.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

/** Events the ForkLaunch platform delivers (payments, email, sms, …). */
export const platformEventsRouter = forklaunchRouter(
  '/platform-events',
  schemaValidator,
  openTelemetryCollector
);

export const receivePlatformEventRoute = platformEventsRouter.post(
  '/:feature',
  receivePlatformEvent
);
