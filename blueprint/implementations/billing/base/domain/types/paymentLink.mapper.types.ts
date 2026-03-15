import { EntityManager, EntitySchema } from '@mikro-orm/core';
import { BasePaymentLinkDtos } from './baseBillingDto.types';
import { BasePaymentLinkEntities } from './baseBillingEntity.types';

export type PaymentLinkMappers<
  PaymentMethodEnum,
  CurrencyEnum,
  StatusEnum,
  MapperEntities extends BasePaymentLinkEntities<
    PaymentMethodEnum,
    CurrencyEnum,
    StatusEnum
  >,
  MapperDomains extends BasePaymentLinkDtos<
    PaymentMethodEnum,
    CurrencyEnum,
    StatusEnum
  >
> = {
  PaymentLinkMapper: {
    entity: EntitySchema;
    toDto: (
      entity: MapperEntities['PaymentLinkMapper']
    ) => Promise<MapperDomains['PaymentLinkMapper']>;
  };
  CreatePaymentLinkMapper: {
    entity: EntitySchema;
    toEntity: (
      dto: MapperDomains['CreatePaymentLinkMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<MapperEntities['CreatePaymentLinkMapper']>;
  };
  UpdatePaymentLinkMapper: {
    entity: EntitySchema;
    toEntity: (
      dto: MapperDomains['UpdatePaymentLinkMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<MapperEntities['UpdatePaymentLinkMapper']>;
  };
};
