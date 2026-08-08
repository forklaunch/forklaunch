import { EntityManager, InferEntity } from '@mikro-orm/core';
import { BasePaymentDtos } from './baseEcommerceDto.types';
import { BasePaymentEntities } from './baseEcommerceEntity.types';

export type PaymentMappers<
  MapperEntities extends BasePaymentEntities,
  MapperDomains extends BasePaymentDtos = BasePaymentDtos
> = {
  PaymentMapper: {
    entity: MapperEntities['PaymentMapper'];
    toDto: (
      entity: InferEntity<MapperEntities['PaymentMapper']>
    ) => Promise<MapperDomains['PaymentMapper']>;
  };
  CreatePaymentMapper: {
    entity: MapperEntities['CreatePaymentMapper'];
    toEntity: (
      dto: MapperDomains['CreatePaymentMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['CreatePaymentMapper']>>;
  };
};
