import { EntityManager } from '@mikro-orm/core';
import { InventoryServiceParameters } from '../types/inventory.service.types';

export interface InventoryService<
  Params extends InventoryServiceParameters = InventoryServiceParameters
> {
  /** Called once per variant at catalog-import time (ECOM-04/import door). */
  createInventory: (
    inventoryDto: Params['CreateInventoryDto'],
    em?: EntityManager
  ) => Promise<Params['InventoryDto']>;
  getInventory: (
    variantIdDto: { variantId: string },
    em?: EntityManager
  ) => Promise<Params['InventoryDto']>;
  adjustStock: (
    adjustDto: Params['AdjustStockDto'],
    em?: EntityManager
  ) => Promise<Params['InventoryDto']>;
  checkStock: (
    stockCheckDto: Params['StockCheckDto'],
    em?: EntityManager
  ) => Promise<Params['StockCheckResultDto']>;
}
