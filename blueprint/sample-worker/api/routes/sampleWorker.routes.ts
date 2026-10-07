import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  sampleWorkerGet,
  sampleWorkerPost
} from '../controllers/sampleWorker.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const sampleWorkerRouter = forklaunchRouter(
  '/sample-worker',
  schemaValidator,
  openTelemetryCollector
);

export const sampleWorkerGetRoute = sampleWorkerRouter.get(
  '/:id',
  sampleWorkerGet
);
export const sampleWorkerPostRoute = sampleWorkerRouter.post(
  '/',
  sampleWorkerPost
);
