import { EntityManager, InferEntity } from '@mikro-orm/core';
import Stripe from 'stripe';
import { StripeCheckoutSessionDtos } from './stripe.dto.types';
import { StripeCheckoutSessionEntities } from './stripe.entity.types';

export type StripeCheckoutSessionMappers<
  StatusEnum,
  Entities extends StripeCheckoutSessionEntities,
  Dto extends StripeCheckoutSessionDtos<StatusEnum>
> = {
  CheckoutSessionMapper: {
    entity: Entities['CheckoutSessionMapper'];
    toDto: (
      entity: InferEntity<Entities['CheckoutSessionMapper']>
    ) => Promise<Dto['CheckoutSessionMapper']>;
    /** Opt-in configured UUID/cache profile; does not prove provider account ownership. */
    readCachedDto?: (
      existing: Dto['CheckoutSessionMapper'],
      retrieve: (providerId: string) => Promise<Stripe.Checkout.Session>
    ) => Promise<Dto['CheckoutSessionMapper']>;
  };
  CreateCheckoutSessionMapper: {
    /** Provider effects already occurred; validate shape before local persistence. */
    validateProviderCreate?: (provider: Stripe.Checkout.Session) => Promise<void>;
    /** Pure configured validation before any provider creation effects. */
    validateCreate?: (dto: Dto['CreateCheckoutSessionMapper']) => Promise<void>;
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
