import { forklaunchRouter, schemaValidator } from '../../schema';
import { ci, tokens } from '../../bootstrapper';
import {
  adjustStock,
  checkStock,
  getInventory
} from '../controllers/inventory.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const inventoryRouter = forklaunchRouter(
  '/inventory',
  schemaValidator,
  openTelemetryCollector
);

export const checkStockRoute = inventoryRouter.post('/check', checkStock);
export const adjustStockRoute = inventoryRouter.put('/adjust', adjustStock);
export const getInventoryRoute = inventoryRouter.get('/:variantId', getInventory);
