import { EntityManager, InferEntity } from '@mikro-orm/core';
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
    entity: MapperEntities['PaymentLinkMapper'];
    toDto: (
      entity: InferEntity<MapperEntities['PaymentLinkMapper']>
    ) => Promise<MapperDomains['PaymentLinkMapper']>;
  };
  CreatePaymentLinkMapper: {
    entity: MapperEntities['CreatePaymentLinkMapper'];
    toEntity: (
      dto: MapperDomains['CreatePaymentLinkMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['CreatePaymentLinkMapper']>>;
  };
  UpdatePaymentLinkMapper: {
    entity: MapperEntities['UpdatePaymentLinkMapper'];
    toEntity: (
      dto: MapperDomains['UpdatePaymentLinkMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['UpdatePaymentLinkMapper']>>;
  };
};
