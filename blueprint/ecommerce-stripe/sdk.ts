import { SchemaValidator } from './schema';
import { MapToSdk } from '@forklaunch/core/http';
import {
  adjustStock,
  checkStock,
  createVariant,
  deleteVariant,
  getInventory,
  getVariant,
  listVariants,
  listVariantsByProduct,
  updateVariant
} from './api/controllers';

//! SDK surface is built up incrementally as each entity's PR lands.
export type EcommerceSdk = {
  variant: {
    createVariant: typeof createVariant;
    getVariant: typeof getVariant;
    listVariantsByProduct: typeof listVariantsByProduct;
    updateVariant: typeof updateVariant;
    deleteVariant: typeof deleteVariant;
    listVariants: typeof listVariants;
  };
  inventory: {
    getInventory: typeof getInventory;
    adjustStock: typeof adjustStock;
    checkStock: typeof checkStock;
  };
};

export const ecommerceSdkClient = {
  variant: {
    createVariant,
    getVariant,
    listVariantsByProduct,
    updateVariant,
    deleteVariant,
    listVariants
  },
  inventory: {
    getInventory,
    adjustStock,
    checkStock
  }
} satisfies EcommerceSdk;

export type EcommerceSdkClient = MapToSdk<SchemaValidator, EcommerceSdk>;
