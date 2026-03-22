import { EntityManager, InferEntity } from '@mikro-orm/core';
import Stripe from 'stripe';
import { StripeCheckoutSessionDtos } from './stripe.dto.types';
import { StripeCheckoutSessionEntities } from './stripe.entity.types';

export type StripeCheckoutSessionMappers<
  StatusEnum,
  Entities extends StripeCheckoutSessionEntities<StatusEnum>,
  Dto extends StripeCheckoutSessionDtos<StatusEnum>
> = {
  CheckoutSessionMapper: {
    entity: Entities['CheckoutSessionMapper'];
    toDto: (
      entity: InferEntity<Entities['CheckoutSessionMapper']>
    ) => Promise<Dto['CheckoutSessionMapper']>;
  };
  CreateCheckoutSessionMapper: {
    entity: Entities['CreateCheckoutSessionMapper'];
    toEntity: (
      dto: Dto['CreateCheckoutSessionMapper'],
      em: EntityManager,
      stripeSession: Stripe.Checkout.Session,
      ...args: unknown[]
    ) => Promise<InferEntity<Entities['CreateCheckoutSessionMapper']>>;
  };
  UpdateCheckoutSessionMapper: {
    entity: Entities['UpdateCheckoutSessionMapper'];
    toEntity: (
      dto: Dto['UpdateCheckoutSessionMapper'],
      em: EntityManager,
      stripeSession: Stripe.Checkout.Session,
      ...args: unknown[]
    ) => Promise<InferEntity<Entities['UpdateCheckoutSessionMapper']>>;
  };
};
