import { complianceContract } from '../contracts/compliance.contract';
import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import {
  eraseUserData,
  exportUserData
} from '../controllers/compliance.controller';
import { ci, tokens } from '../../bootstrapper';

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
