import { EntityManager, InferEntity } from '@mikro-orm/core';
import { BasePlanDtos } from './baseBillingDto.types';
import { BasePlanEntities } from './baseBillingEntity.types';

export type PlanMappers<
  PlanCadenceEnum,
  CurrencyEnum,
  BillingProviderEnum,
  MapperEntities extends BasePlanEntities<
    PlanCadenceEnum,
    CurrencyEnum,
    BillingProviderEnum
  >,
  MapperDomains extends BasePlanDtos<
    PlanCadenceEnum,
    CurrencyEnum,
    BillingProviderEnum
  >
> = {
  PlanMapper: {
    entity: MapperEntities['PlanMapper'];
    toDto: (
      entity: InferEntity<MapperEntities['PlanMapper']>
    ) => Promise<MapperDomains['PlanMapper']>;
  };
  CreatePlanMapper: {
    entity: MapperEntities['CreatePlanMapper'];
    toEntity: (
      dto: MapperDomains['CreatePlanMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['CreatePlanMapper']>>;
  };
  UpdatePlanMapper: {
    entity: MapperEntities['UpdatePlanMapper'];
    toEntity: (
      dto: MapperDomains['UpdatePlanMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['UpdatePlanMapper']>>;
  };
};
