import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import { handleStatusCallback } from '../controllers/webhook.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const webhookRouter = forklaunchRouter(
  '/webhook',
  schemaValidator,
  openTelemetryCollector
);

export const handleStatusCallbackRoute = webhookRouter.post(
  '/',
  handleStatusCallback
);
