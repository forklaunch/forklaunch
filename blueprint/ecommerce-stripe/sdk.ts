import { SchemaValidator } from './schema';
import { MapToSdk } from '@forklaunch/core/http';
import {
  adjustStock,
  checkStock,
  createProduct,
  createVariant,
  deleteProduct,
  deleteVariant,
  getInventory,
  getProduct,
  getProductByHandle,
  getVariant,
  importCatalog,
  listProducts,
  listVariants,
  listVariantsByProduct,
  updateProduct,
  updateVariant
} from './api/controllers';

export type EcommerceSdk = {
  product: {
    createProduct: typeof createProduct;
    getProduct: typeof getProduct;
    getProductByHandle: typeof getProductByHandle;
    updateProduct: typeof updateProduct;
    deleteProduct: typeof deleteProduct;
    listProducts: typeof listProducts;
  };
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
  catalogImport: {
    importCatalog: typeof importCatalog;
  };
};

export const ecommerceSdkClient = {
  product: {
    createProduct,
    getProduct,
    getProductByHandle,
    updateProduct,
    deleteProduct,
    listProducts
  },
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
  },
  catalogImport: {
    importCatalog
  }
} satisfies EcommerceSdk;

export type EcommerceSdkClient = MapToSdk<SchemaValidator, EcommerceSdk>;
