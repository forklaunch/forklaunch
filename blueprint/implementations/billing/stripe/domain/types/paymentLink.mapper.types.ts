import { EntityManager, EntitySchema } from '@mikro-orm/core';
import Stripe from 'stripe';
import { StripePaymentLinkDtos } from './stripe.dto.types';
import { StripePaymentLinkEntities } from './stripe.entity.types';

export type StripePaymentLinkMappers<
  StatusEnum,
  Entities extends StripePaymentLinkEntities<StatusEnum>,
  Dto extends StripePaymentLinkDtos<StatusEnum>
> = {
  PaymentLinkMapper: {
    entity: EntitySchema;
    toDto: (
      entity: Entities['PaymentLinkMapper']
    ) => Promise<Dto['PaymentLinkMapper']>;
  };
  CreatePaymentLinkMapper: {
    entity: EntitySchema;
    toEntity: (
      dto: Dto['CreatePaymentLinkMapper'],
      em: EntityManager,
      stripePaymentLink: Stripe.PaymentLink,
      ...args: unknown[]
    ) => Promise<Entities['CreatePaymentLinkMapper']>;
  };
  UpdatePaymentLinkMapper: {
    entity: EntitySchema;
    toEntity: (
      dto: Dto['UpdatePaymentLinkMapper'],
      em: EntityManager,
      stripePaymentLink: Stripe.PaymentLink,
      ...args: unknown[]
    ) => Promise<Entities['UpdatePaymentLinkMapper']>;
  };
};
