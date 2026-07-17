import { EntityManager, InferEntity } from '@mikro-orm/core';
import { BaseProductDtos } from './baseEcommerceDto.types';
import { BaseProductEntities } from './baseEcommerceEntity.types';

export type ProductMappers<
  MapperEntities extends BaseProductEntities,
  MapperDomains extends BaseProductDtos = BaseProductDtos
> = {
  ProductMapper: {
    entity: MapperEntities['ProductMapper'];
    toDto: (
      entity: InferEntity<MapperEntities['ProductMapper']>
    ) => Promise<MapperDomains['ProductMapper']>;
  };
  CreateProductMapper: {
    entity: MapperEntities['CreateProductMapper'];
    toEntity: (
      dto: MapperDomains['CreateProductMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['CreateProductMapper']>>;
  };
  UpdateProductMapper: {
    entity: MapperEntities['UpdateProductMapper'];
    toEntity: (
      dto: MapperDomains['UpdateProductMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['UpdateProductMapper']>>;
  };
};
