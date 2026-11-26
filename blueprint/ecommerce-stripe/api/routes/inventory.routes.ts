import { forklaunchRouter, schemaValidator } from '../../schema';
import { ci, tokens } from '../../bootstrapper';
import {
  adjustStock,
  checkStock,
  createInventory,
  getInventory
} from '../controllers/inventory.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const inventoryRouter = forklaunchRouter(
  '/inventory',
  schemaValidator,
  openTelemetryCollector
);

export const createInventoryRoute = inventoryRouter.post('/', createInventory);
export const checkStockRoute = inventoryRouter.post('/check', checkStock);
export const adjustStockRoute = inventoryRouter.put('/adjust', adjustStock);
export const getInventoryRoute = inventoryRouter.get('/:variantId', getInventory);
