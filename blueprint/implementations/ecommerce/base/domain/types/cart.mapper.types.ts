import { EntityManager, InferEntity } from '@mikro-orm/core';
import { BaseCartDtos } from './baseEcommerceDto.types';
import { BaseCartEntities } from './baseEcommerceEntity.types';

export type CartMappers<
  MapperEntities extends BaseCartEntities,
  MapperDomains extends BaseCartDtos = BaseCartDtos
> = {
  CartMapper: {
    entity: MapperEntities['CartMapper'];
    toDto: (
      entity: InferEntity<MapperEntities['CartMapper']>
    ) => Promise<MapperDomains['CartMapper']>;
  };
  CreateCartMapper: {
    entity: MapperEntities['CreateCartMapper'];
    toEntity: (
      dto: MapperDomains['CreateCartMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['CreateCartMapper']>>;
  };
  UpdateCartMapper: {
    entity: MapperEntities['UpdateCartMapper'];
    toEntity: (
      dto: MapperDomains['UpdateCartMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['UpdateCartMapper']>>;
  };
};
