import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import { search } from '../controllers/search.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const searchRouter = forklaunchRouter(
  '/search',
  schemaValidator,
  openTelemetryCollector
);

export const searchRoute = searchRouter.get('/', search);
