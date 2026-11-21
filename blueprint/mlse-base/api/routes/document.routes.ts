import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import { getDocument } from '../controllers/document.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const documentRouter = forklaunchRouter(
  '/document',
  schemaValidator,
  openTelemetryCollector
);

export const getDocumentRoute = documentRouter.get('/:id', getDocument);
