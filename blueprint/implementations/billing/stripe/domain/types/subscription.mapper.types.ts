import { EntityManager, EntitySchema } from '@mikro-orm/core';
import Stripe from 'stripe';
import { StripeSubscriptionDtos } from './stripe.dto.types';
import { StripeSubscriptionEntities } from './stripe.entity.types';

export type StripeSubscriptionMappers<
  PartyType,
  Entities extends StripeSubscriptionEntities<PartyType>,
  Dto extends StripeSubscriptionDtos<PartyType>
> = {
  SubscriptionMapper: {
    entity: EntitySchema;
    toDto: (
      entity: Entities['SubscriptionMapper']
    ) => Promise<Dto['SubscriptionMapper']>;
  };
  CreateSubscriptionMapper: {
    entity: EntitySchema;
    toEntity: (
      dto: Dto['CreateSubscriptionMapper'],
      em: EntityManager,
      stripeSubscription: Stripe.Subscription,
      ...args: unknown[]
    ) => Promise<Entities['CreateSubscriptionMapper']>;
  };
  UpdateSubscriptionMapper: {
    entity: EntitySchema;
    toEntity: (
      dto: Dto['UpdateSubscriptionMapper'],
      em: EntityManager,
      stripeSubscription: Stripe.Subscription,
      ...args: unknown[]
    ) => Promise<Entities['UpdateSubscriptionMapper']>;
  };
};
