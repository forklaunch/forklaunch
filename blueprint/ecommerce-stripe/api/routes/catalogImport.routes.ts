import { forklaunchRouter, schemaValidator } from '../../schema';
import { ci, tokens } from '../../bootstrapper';
import { importCatalog } from '../controllers/catalogImport.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const catalogImportRouter = forklaunchRouter(
  '/catalog-import',
  schemaValidator,
  openTelemetryCollector
);

export const importCatalogRoute = catalogImportRouter.post(
  '/',
  importCatalog
);
