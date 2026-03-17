import { EntityManager, EntitySchema } from '@mikro-orm/core';
import Stripe from 'stripe';
import { StripeBillingPortalDtos } from './stripe.dto.types';
import { StripeBillingPortalEntities } from './stripe.entity.types';

export type StripeBillingPortalMappers<
  Entities extends StripeBillingPortalEntities,
  Dto extends StripeBillingPortalDtos
> = {
  BillingPortalMapper: {
    entity: EntitySchema<any>;
    toDto: (
      entity: Entities['BillingPortalMapper']
    ) => Promise<Dto['BillingPortalMapper']>;
  };
  CreateBillingPortalMapper: {
    entity: EntitySchema<any>;
    toEntity: (
      dto: Dto['CreateBillingPortalMapper'],
      em: EntityManager,
      stripeSession: Stripe.BillingPortal.Session,
      ...args: unknown[]
    ) => Promise<Entities['CreateBillingPortalMapper']>;
  };
  UpdateBillingPortalMapper: {
    entity: EntitySchema<any>;
    toEntity: (
      dto: Dto['UpdateBillingPortalMapper'],
      em: EntityManager,
      stripeSession: Stripe.BillingPortal.Session,
      ...args: unknown[]
    ) => Promise<Entities['UpdateBillingPortalMapper']>;
  };
};
