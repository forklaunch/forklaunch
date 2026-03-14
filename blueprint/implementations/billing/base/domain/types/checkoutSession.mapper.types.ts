import { EntityManager, EntitySchema } from '@mikro-orm/core';
import { BaseCheckoutSessionDtos } from './baseBillingDto.types';
import { BaseCheckoutSessionEntities } from './baseBillingEntity.types';

export type CheckoutSessionMappers<
  PaymentMethodEnum,
  CurrencyEnum,
  StatusEnum,
  MapperEntities extends BaseCheckoutSessionEntities<
    PaymentMethodEnum,
    CurrencyEnum,
    StatusEnum
  >,
  MapperDomains extends BaseCheckoutSessionDtos<
    PaymentMethodEnum,
    CurrencyEnum,
    StatusEnum
  >
> = {
  CheckoutSessionMapper: {
    entity: EntitySchema;
    toDto: (
      entity: MapperEntities['CheckoutSessionMapper']
    ) => Promise<MapperDomains['CheckoutSessionMapper']>;
  };
  CreateCheckoutSessionMapper: {
    entity: EntitySchema;
    toEntity: (
      dto: MapperDomains['CreateCheckoutSessionMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<MapperEntities['CreateCheckoutSessionMapper']>;
  };
  UpdateCheckoutSessionMapper: {
    entity: EntitySchema;
    toEntity: (
      dto: MapperDomains['UpdateCheckoutSessionMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<MapperEntities['UpdateCheckoutSessionMapper']>;
  };
};
