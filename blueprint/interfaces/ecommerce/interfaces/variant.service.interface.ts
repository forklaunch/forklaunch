import { EntityManager } from '@mikro-orm/core';
import { VariantServiceParameters } from '../types/variant.service.types';

export interface VariantService<
  Params extends VariantServiceParameters = VariantServiceParameters
> {
  createVariant: (
    variantDto: Params['CreateVariantDto'],
    em?: EntityManager
  ) => Promise<Params['VariantDto']>;
  getVariant: (
    idDto: Params['IdDto'],
    em?: EntityManager
  ) => Promise<Params['VariantDto']>;
  updateVariant: (
    variantDto: Params['UpdateVariantDto'],
    em?: EntityManager
  ) => Promise<Params['VariantDto']>;
  /** Throws if no variant with this externalId exists — used to decide create-vs-update on import. */
  getVariantByExternalId: (
    externalIdDto: { externalId: string },
    em?: EntityManager
  ) => Promise<Params['VariantDto']>;
  deleteVariant: (idDto: Params['IdDto'], em?: EntityManager) => Promise<void>;
  listVariants: (
    idsDto?: Params['IdsDto'],
    em?: EntityManager
  ) => Promise<Params['VariantDto'][]>;
  listVariantsByProduct: (
    productIdDto: { productId: string },
    em?: EntityManager
  ) => Promise<Params['VariantDto'][]>;
}
