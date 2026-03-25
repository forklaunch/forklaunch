import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  eraseUserData,
  exportUserData
} from '../controllers/compliance.controller';

const openTelemetryCollector = ci.resolve(tokens.OpenTelemetryCollector);

export const complianceRouter = forklaunchRouter(
  '/compliance',
  schemaValidator,
  openTelemetryCollector
);

export const eraseUserDataRoute = complianceRouter.delete(
  '/erase/:userId',
  eraseUserData
);
export const exportUserDataRoute = complianceRouter.get(
  '/export/:userId',
  exportUserData
);
