import { BasePaymentDtos, BasePaymentEntities } from '@forklaunch/implementation-ecommerce-base/types';
import { EntityManager, InferEntity } from '@mikro-orm/core';
import Stripe from 'stripe';

/**
 * Stripe-specific override of PaymentMappers — CreatePaymentMapper.toEntity
 * takes the concrete Stripe.PaymentIntent (created by StripePaymentService
 * before persistence), not the base's generic ...args: unknown[]. Same
 * pattern as StripePlanMappers in billing overriding the generic PlanMappers.
 */
export type StripePaymentMappers<
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
      paymentIntent: Stripe.PaymentIntent,
      ...args: unknown[]
    ) => Promise<InferEntity<Entities['CreatePaymentMapper']>>;
  };
};
