import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import { listSources, refreshSource } from '../controllers/source.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const sourceRouter = forklaunchRouter(
  '/source',
  schemaValidator,
  openTelemetryCollector
);

export const listSourcesRoute = sourceRouter.get('/', listSources);
export const refreshSourceRoute = sourceRouter.post(
  '/:sourceKey/refresh',
  refreshSource
);
