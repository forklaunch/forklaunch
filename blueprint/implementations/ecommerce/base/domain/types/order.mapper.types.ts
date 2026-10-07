import { EntityManager, InferEntity } from '@mikro-orm/core';
import { BaseOrderDtos } from './baseEcommerceDto.types';
import { BaseOrderEntities } from './baseEcommerceEntity.types';

export type OrderMappers<
  MapperEntities extends BaseOrderEntities,
  MapperDomains extends BaseOrderDtos = BaseOrderDtos
> = {
  OrderMapper: {
    entity: MapperEntities['OrderMapper'];
    toDto: (
      entity: InferEntity<MapperEntities['OrderMapper']>
    ) => Promise<MapperDomains['OrderMapper']>;
  };
  CreateOrderMapper: {
    entity: MapperEntities['CreateOrderMapper'];
    toEntity: (
      dto: MapperDomains['CreateOrderMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['CreateOrderMapper']>>;
  };
  UpdateOrderMapper: {
    entity: MapperEntities['UpdateOrderMapper'];
    toEntity: (
      dto: MapperDomains['UpdateOrderMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['UpdateOrderMapper']>>;
  };
};
