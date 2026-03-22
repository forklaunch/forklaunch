import { EntityManager, InferEntity } from '@mikro-orm/core';
import { BaseSubscriptionDtos } from './baseBillingDto.types';
import { BaseSubscriptionEntities } from './baseBillingEntity.types';

export type SubscriptionMappers<
  PartyType,
  BillingProviderType,
  MapperEntities extends BaseSubscriptionEntities<
    PartyType,
    BillingProviderType
  >,
  MapperDomains extends BaseSubscriptionDtos<PartyType, BillingProviderType>
> = {
  SubscriptionMapper: {
    entity: MapperEntities['SubscriptionMapper'];
    toDto: (
      entity: InferEntity<MapperEntities['SubscriptionMapper']>
    ) => Promise<MapperDomains['SubscriptionMapper']>;
  };
  CreateSubscriptionMapper: {
    entity: MapperEntities['CreateSubscriptionMapper'];
    toEntity: (
      dto: MapperDomains['CreateSubscriptionMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['CreateSubscriptionMapper']>>;
  };
  UpdateSubscriptionMapper: {
    entity: MapperEntities['UpdateSubscriptionMapper'];
    toEntity: (
      dto: MapperDomains['UpdateSubscriptionMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['UpdateSubscriptionMapper']>>;
  };
};
