import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  approveTopic,
  createContentLicense,
  flagContent,
  listContentFlags,
  listContentLicenses,
  listSourceAccess,
  registerLicensedSource,
  resolveContentFlag,
  revokeContentLicense
} from '../controllers/governance.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const governanceRouter = forklaunchRouter(
  '/admin',
  schemaValidator,
  openTelemetryCollector
);

export const listSourceAccessRoute = governanceRouter.get('/source-access', listSourceAccess);
export const registerLicensedSourceRoute = governanceRouter.post('/source', registerLicensedSource);
export const listContentLicensesRoute = governanceRouter.get('/content-license', listContentLicenses);
export const createContentLicenseRoute = governanceRouter.post('/content-license', createContentLicense);
export const revokeContentLicenseRoute = governanceRouter.post('/content-license/:id/revoke', revokeContentLicense);
export const flagContentRoute = governanceRouter.post('/content-flag', flagContent);
export const listContentFlagsRoute = governanceRouter.get('/content-flag', listContentFlags);
export const resolveContentFlagRoute = governanceRouter.post('/content-flag/:id/resolve', resolveContentFlag);
export const approveTopicRoute = governanceRouter.post('/topic/:slug/approve', approveTopic);
