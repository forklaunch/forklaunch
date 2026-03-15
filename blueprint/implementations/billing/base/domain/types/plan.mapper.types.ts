import { EntityManager, EntitySchema } from '@mikro-orm/core';
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
    entity: EntitySchema;
    toDto: (
      entity: MapperEntities['PlanMapper']
    ) => Promise<MapperDomains['PlanMapper']>;
  };
  CreatePlanMapper: {
    entity: EntitySchema;
    toEntity: (
      dto: MapperDomains['CreatePlanMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<MapperEntities['CreatePlanMapper']>;
  };
  UpdatePlanMapper: {
    entity: EntitySchema;
    toEntity: (
      dto: MapperDomains['UpdatePlanMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<MapperEntities['UpdatePlanMapper']>;
  };
};
