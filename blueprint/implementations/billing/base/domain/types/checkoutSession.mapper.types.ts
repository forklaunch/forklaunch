import { EntityManager, InferEntity } from '@mikro-orm/core';
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
    entity: MapperEntities['CheckoutSessionMapper'];
    toDto: (
      entity: InferEntity<MapperEntities['CheckoutSessionMapper']>
    ) => Promise<MapperDomains['CheckoutSessionMapper']>;
  };
  CreateCheckoutSessionMapper: {
    entity: MapperEntities['CreateCheckoutSessionMapper'];
    toEntity: (
      dto: MapperDomains['CreateCheckoutSessionMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['CreateCheckoutSessionMapper']>>;
  };
  UpdateCheckoutSessionMapper: {
    entity: MapperEntities['UpdateCheckoutSessionMapper'];
    toEntity: (
      dto: MapperDomains['UpdateCheckoutSessionMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['UpdateCheckoutSessionMapper']>>;
  };
};
