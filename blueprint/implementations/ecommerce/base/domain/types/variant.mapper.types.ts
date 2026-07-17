import { EntityManager, InferEntity } from '@mikro-orm/core';
import { BaseVariantDtos } from './baseEcommerceDto.types';
import { BaseVariantEntities } from './baseEcommerceEntity.types';

export type VariantMappers<
  MapperEntities extends BaseVariantEntities,
  MapperDomains extends BaseVariantDtos = BaseVariantDtos
> = {
  VariantMapper: {
    entity: MapperEntities['VariantMapper'];
    toDto: (
      entity: InferEntity<MapperEntities['VariantMapper']>
    ) => Promise<MapperDomains['VariantMapper']>;
  };
  CreateVariantMapper: {
    entity: MapperEntities['CreateVariantMapper'];
    toEntity: (
      dto: MapperDomains['CreateVariantMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['CreateVariantMapper']>>;
  };
  UpdateVariantMapper: {
    entity: MapperEntities['UpdateVariantMapper'];
    toEntity: (
      dto: MapperDomains['UpdateVariantMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['UpdateVariantMapper']>>;
  };
};
