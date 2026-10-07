import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  describeCodeSet,
  lookupProcedureCode
} from '../controllers/codeSet.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const codeSetRouter = forklaunchRouter(
  '/codeSet',
  schemaValidator,
  openTelemetryCollector
);

export const describeCodeSetRoute = codeSetRouter.get('/', describeCodeSet);
export const lookupProcedureCodeRoute = codeSetRouter.get(
  '/:code',
  lookupProcedureCode
);
