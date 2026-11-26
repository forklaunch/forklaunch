import { complianceContract } from '../contracts/compliance.contract';
import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  eraseUserData,
  exportUserData
} from '../controllers/compliance.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const complianceRouter = forklaunchRouter(
  complianceContract.basePath,
  schemaValidator,
  openTelemetryCollector
);

export const eraseUserDataRoute = complianceRouter.delete(
  complianceContract.eraseUserData.path,
  eraseUserData
);
export const exportUserDataRoute = complianceRouter.get(
  complianceContract.exportUserData.path,
  exportUserData
);
