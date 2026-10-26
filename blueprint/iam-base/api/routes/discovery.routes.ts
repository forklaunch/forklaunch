import { discoveryContract } from '../contracts/discovery.contract';
import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import { getAuthMethods } from '../controllers/discovery.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const discoveryRouter = forklaunchRouter(
  discoveryContract.basePath,
  schemaValidator,
  openTelemetryCollector
);

export const getAuthMethodsRoute = discoveryRouter.get(
  discoveryContract.getAuthMethods.path,
  getAuthMethods
);
