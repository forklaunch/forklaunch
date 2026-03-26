import { forklaunchRouter, schemaValidator } from '@{{app_name}}/core';
import { eraseUserData, exportUserData } from '../controllers/compliance.controller';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.{{otel_token}});

export const complianceRouter = forklaunchRouter(
  '/compliance',
  schemaValidator,
  openTelemetryCollector
);

export const eraseUserDataRoute = complianceRouter.delete('/erase/:userId', eraseUserData);
export const exportUserDataRoute = complianceRouter.get('/export/:userId', exportUserData);
