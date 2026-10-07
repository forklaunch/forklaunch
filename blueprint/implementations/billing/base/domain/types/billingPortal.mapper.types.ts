import { EntityManager, InferEntity } from '@mikro-orm/core';
import { BaseBillingDtos } from './baseBillingDto.types';
import { BaseBillingEntities } from './baseBillingEntity.types';

export type BillingPortalMappers<
  MapperEntities extends BaseBillingEntities,
  MapperDomains extends BaseBillingDtos
> = {
  BillingPortalMapper: {
    entity: MapperEntities['BillingPortalMapper'];
    toDto: (
      entity: InferEntity<MapperEntities['BillingPortalMapper']>
    ) => Promise<MapperDomains['BillingPortalMapper']>;
  };
  CreateBillingPortalMapper: {
    entity: MapperEntities['CreateBillingPortalMapper'];
    toEntity: (
      dto: MapperDomains['CreateBillingPortalMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['CreateBillingPortalMapper']>>;
  };
  UpdateBillingPortalMapper: {
    entity: MapperEntities['UpdateBillingPortalMapper'];
    toEntity: (
      dto: MapperDomains['UpdateBillingPortalMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['UpdateBillingPortalMapper']>>;
  };
};
