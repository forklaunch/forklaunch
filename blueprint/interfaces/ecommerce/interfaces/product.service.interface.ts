import { EntityManager } from '@mikro-orm/core';
import { ProductServiceParameters } from '../types/product.service.types';

export interface ProductService<
  Params extends ProductServiceParameters = ProductServiceParameters
> {
  createProduct: (
    productDto: Params['CreateProductDto'],
    em?: EntityManager
  ) => Promise<Params['ProductDto']>;
  getProduct: (
    idDto: Params['IdDto'],
    em?: EntityManager
  ) => Promise<Params['ProductDto']>;
  getProductByHandle: (
    handleDto: { handle: string },
    em?: EntityManager
  ) => Promise<Params['ProductDto']>;
  /** Throws if no product with this externalId exists — used to decide create-vs-update on import. */
  getProductByExternalId: (
    externalIdDto: { externalId: string },
    em?: EntityManager
  ) => Promise<Params['ProductDto']>;
  updateProduct: (
    productDto: Params['UpdateProductDto'],
    em?: EntityManager
  ) => Promise<Params['ProductDto']>;
  deleteProduct: (idDto: Params['IdDto'], em?: EntityManager) => Promise<void>;
  /** Catalog search/filter — all fields optional and combinable. */
  listProducts: (
    searchDto?: Params['SearchDto'],
    em?: EntityManager
  ) => Promise<Params['ProductDto'][]>;
}
