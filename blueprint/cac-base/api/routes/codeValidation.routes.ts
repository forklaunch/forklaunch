import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  validateHcpcsCode,
  validateIcd10Code
} from '../controllers/codeValidation.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const codeValidationRouter = forklaunchRouter(
  '/codeValidation',
  schemaValidator,
  openTelemetryCollector
);

export const validateIcd10CodeRoute = codeValidationRouter.get(
  '/icd10/:code',
  validateIcd10Code
);
export const validateHcpcsCodeRoute = codeValidationRouter.get(
  '/hcpcs/:code',
  validateHcpcsCode
);
