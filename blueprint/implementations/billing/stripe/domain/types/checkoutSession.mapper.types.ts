import { EntityManager, EntitySchema } from '@mikro-orm/core';
import Stripe from 'stripe';
import { StripeCheckoutSessionDtos } from './stripe.dto.types';
import { StripeCheckoutSessionEntities } from './stripe.entity.types';

export type StripeCheckoutSessionMappers<
  StatusEnum,
  Entities extends StripeCheckoutSessionEntities<StatusEnum>,
  Dto extends StripeCheckoutSessionDtos<StatusEnum>
> = {
  CheckoutSessionMapper: {
    entity: EntitySchema;
    toDto: (
      entity: Entities['CheckoutSessionMapper']
    ) => Promise<Dto['CheckoutSessionMapper']>;
  };
  CreateCheckoutSessionMapper: {
    entity: EntitySchema;
    toEntity: (
      dto: Dto['CreateCheckoutSessionMapper'],
      em: EntityManager,
      stripeSession: Stripe.Checkout.Session,
      ...args: unknown[]
    ) => Promise<Entities['CreateCheckoutSessionMapper']>;
  };
  UpdateCheckoutSessionMapper: {
    entity: EntitySchema;
    toEntity: (
      dto: Dto['UpdateCheckoutSessionMapper'],
      em: EntityManager,
      stripeSession: Stripe.Checkout.Session,
      ...args: unknown[]
    ) => Promise<Entities['UpdateCheckoutSessionMapper']>;
  };
};
