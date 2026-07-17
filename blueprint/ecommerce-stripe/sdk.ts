import { SchemaValidator } from './schema';
import { MapToSdk } from '@forklaunch/core/http';
import {
  createVariant,
  deleteVariant,
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
};

export const ecommerceSdkClient = {
  variant: {
    createVariant,
    getVariant,
    listVariantsByProduct,
    updateVariant,
    deleteVariant,
    listVariants
  }
} satisfies EcommerceSdk;

export type EcommerceSdkClient = MapToSdk<SchemaValidator, EcommerceSdk>;
