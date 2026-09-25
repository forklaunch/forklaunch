import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import { answer, answerComplete } from '../controllers/answer.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const answerRouter = forklaunchRouter(
  '/answer',
  schemaValidator,
  openTelemetryCollector
);

export const answerRoute = answerRouter.post('/', answer);
export const answerCompleteRoute = answerRouter.post('/complete', answerComplete);
