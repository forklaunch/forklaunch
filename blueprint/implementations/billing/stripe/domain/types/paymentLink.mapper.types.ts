import { EntityManager, InferEntity } from '@mikro-orm/core';
import Stripe from 'stripe';
import { StripePaymentLinkDtos } from './stripe.dto.types';
import { StripePaymentLinkEntities } from './stripe.entity.types';

export type StripePaymentLinkMappers<
  StatusEnum,
  Entities extends StripePaymentLinkEntities<StatusEnum>,
  Dto extends StripePaymentLinkDtos<StatusEnum>
> = {
  PaymentLinkMapper: {
    entity: Entities['PaymentLinkMapper'];
    toDto: (
      entity: InferEntity<Entities['PaymentLinkMapper']>
    ) => Promise<Dto['PaymentLinkMapper']>;
  };
  CreatePaymentLinkMapper: {
    entity: Entities['CreatePaymentLinkMapper'];
    toEntity: (
      dto: Dto['CreatePaymentLinkMapper'],
      em: EntityManager,
      stripePaymentLink: Stripe.PaymentLink,
      ...args: unknown[]
    ) => Promise<InferEntity<Entities['CreatePaymentLinkMapper']>>;
  };
  UpdatePaymentLinkMapper: {
    entity: Entities['UpdatePaymentLinkMapper'];
    toEntity: (
      dto: Dto['UpdatePaymentLinkMapper'],
      em: EntityManager,
      stripePaymentLink: Stripe.PaymentLink,
      ...args: unknown[]
    ) => Promise<InferEntity<Entities['UpdatePaymentLinkMapper']>>;
  };
};
