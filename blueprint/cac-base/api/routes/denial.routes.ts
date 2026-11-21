import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  getDenial,
  listDenials,
  resolveDenial
} from '../controllers/denial.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const denialRouter = forklaunchRouter(
  '/denial',
  schemaValidator,
  openTelemetryCollector
);

export const listDenialsRoute = denialRouter.get('/', listDenials);
export const getDenialRoute = denialRouter.get('/:id', getDenial);
export const resolveDenialRoute = denialRouter.post(
  '/:id/resolve',
  resolveDenial
);
