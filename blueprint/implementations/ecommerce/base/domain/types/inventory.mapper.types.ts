import { EntityManager, InferEntity } from '@mikro-orm/core';
import { BaseInventoryDtos } from './baseEcommerceDto.types';
import { BaseInventoryEntities } from './baseEcommerceEntity.types';

export type InventoryMappers<
  MapperEntities extends BaseInventoryEntities,
  MapperDomains extends BaseInventoryDtos = BaseInventoryDtos
> = {
  InventoryMapper: {
    entity: MapperEntities['InventoryMapper'];
    toDto: (
      entity: InferEntity<MapperEntities['InventoryMapper']>
    ) => Promise<MapperDomains['InventoryMapper']>;
  };
  CreateInventoryMapper: {
    entity: MapperEntities['CreateInventoryMapper'];
    toEntity: (
      dto: MapperDomains['CreateInventoryMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['CreateInventoryMapper']>>;
  };
  UpdateInventoryMapper: {
    entity: MapperEntities['UpdateInventoryMapper'];
    toEntity: (
      dto: MapperDomains['UpdateInventoryMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['UpdateInventoryMapper']>>;
  };
};
