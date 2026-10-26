import { EntityManager, InferEntity } from '@mikro-orm/core';
import { BasePaymentLinkDtos } from './baseBillingDto.types';
import { BasePaymentLinkEntities } from './baseBillingEntity.types';

export type PaymentLinkMappers<
  PaymentMethodEnum,
  CurrencyEnum,
  StatusEnum,
  MapperEntities extends BasePaymentLinkEntities,
  MapperDomains extends BasePaymentLinkDtos<
    PaymentMethodEnum,
    CurrencyEnum,
    StatusEnum
  >
> = {
  PaymentLinkMapper: {
    /** Validate and restore declared DTO fields after a JSON cache read. */
    normalizeCachedDto?: (
      value: MapperDomains['PaymentLinkMapper']
    ) => MapperDomains['PaymentLinkMapper'];
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
    /** Optional local cache-only mapping; never grants provider mutation authority. */
    toCachedDto?: (
      dto: MapperDomains['UpdatePaymentLinkMapper'],
      existing: MapperDomains['PaymentLinkMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<MapperDomains['PaymentLinkMapper']>;
    entity: MapperEntities['UpdatePaymentLinkMapper'];
    toEntity: (
      dto: MapperDomains['UpdatePaymentLinkMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['UpdatePaymentLinkMapper']>>;
  };
};
