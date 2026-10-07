import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import { getAuthMethods } from '../controllers/discovery.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const discoveryRouter = forklaunchRouter(
  '/discovery',
  schemaValidator,
  openTelemetryCollector
);

export const getAuthMethodsRoute = discoveryRouter.get(
  '/auth-methods',
  getAuthMethods
);
