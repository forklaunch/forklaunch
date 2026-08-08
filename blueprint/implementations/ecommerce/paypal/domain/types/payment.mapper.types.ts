import {
  BasePaymentDtos,
  BasePaymentEntities
} from '@forklaunch/implementation-ecommerce-base/types';
import { EntityManager, InferEntity } from '@mikro-orm/core';
import { PaypalOrder } from '../../services/paypal-client';

/**
 * PayPal-specific override of PaymentMappers — CreatePaymentMapper.toEntity
 * takes the concrete PaypalOrder (created before persistence). Mirrors the
 * StripePaymentMappers override; the base's generic ...args: unknown[] is
 * narrowed here to PaypalOrder.
 */
export type PaypalPaymentMappers<
  Entities extends BasePaymentEntities,
  Dto extends BasePaymentDtos
> = {
  PaymentMapper: {
    entity: Entities['PaymentMapper'];
    toDto: (
      entity: InferEntity<Entities['PaymentMapper']>
    ) => Promise<Dto['PaymentMapper']>;
  };
  CreatePaymentMapper: {
    entity: Entities['CreatePaymentMapper'];
    toEntity: (
      dto: Dto['CreatePaymentMapper'],
      em: EntityManager,
      paypalOrder: PaypalOrder,
      ...args: unknown[]
    ) => Promise<InferEntity<Entities['CreatePaymentMapper']>>;
  };
};
