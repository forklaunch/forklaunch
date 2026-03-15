import { EntityManager, EntitySchema } from '@mikro-orm/core';
import { BaseBillingDtos } from './baseBillingDto.types';
import { BaseBillingEntities } from './baseBillingEntity.types';

export type BillingPortalMappers<
  MapperEntities extends BaseBillingEntities,
  MapperDomains extends BaseBillingDtos
> = {
  BillingPortalMapper: {
    entity: EntitySchema;
    toDto: (
      entity: MapperEntities['BillingPortalMapper']
    ) => Promise<MapperDomains['BillingPortalMapper']>;
  };
  CreateBillingPortalMapper: {
    entity: EntitySchema;
    toEntity: (
      dto: MapperDomains['CreateBillingPortalMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<MapperEntities['CreateBillingPortalMapper']>;
  };
  UpdateBillingPortalMapper: {
    entity: EntitySchema;
    toEntity: (
      dto: MapperDomains['UpdateBillingPortalMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<MapperEntities['UpdateBillingPortalMapper']>;
  };
};
