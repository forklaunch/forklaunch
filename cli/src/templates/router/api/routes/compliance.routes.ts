import { complianceContract } from '../contracts/compliance.contract';
import { forklaunchRouter, schemaValidator } from '@{{app_name}}/core';
import { eraseUserData, exportUserData } from '../controllers/compliance.controller';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.{{otel_token}});

export const complianceRouter = forklaunchRouter(
  complianceContract.basePath,
  schemaValidator,
  openTelemetryCollector
);

export const eraseUserDataRoute = complianceRouter.delete(complianceContract.delete.path, eraseUserData);
export const exportUserDataRoute = complianceRouter.get(complianceContract.get.path, exportUserData);
