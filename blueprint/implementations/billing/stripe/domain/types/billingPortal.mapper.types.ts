import { EntityManager, InferEntity } from '@mikro-orm/core';
import Stripe from 'stripe';
import { StripeBillingPortalDtos } from './stripe.dto.types';
import { StripeBillingPortalEntities } from './stripe.entity.types';

export type StripeBillingPortalMappers<
  Entities extends StripeBillingPortalEntities,
  Dto extends StripeBillingPortalDtos
> = {
  BillingPortalMapper: {
    entity: Entities['BillingPortalMapper'];
    toDto: (
      entity: InferEntity<Entities['BillingPortalMapper']>
    ) => Promise<Dto['BillingPortalMapper']>;
  };
  CreateBillingPortalMapper: {
    /** Provider effects already occurred; validate shape before local persistence. */
    validateProviderCreate?: (provider: Stripe.BillingPortal.Session) => Promise<void>;
    /** Pure configured validation before any provider creation effects. */
    validateCreate?: (dto: Dto['CreateBillingPortalMapper']) => Promise<void>;
    entity: Entities['CreateBillingPortalMapper'];
    toEntity: (
      dto: Dto['CreateBillingPortalMapper'],
      em: EntityManager,
      stripeSession: Stripe.BillingPortal.Session,
      ...args: unknown[]
    ) => Promise<InferEntity<Entities['CreateBillingPortalMapper']>>;
  };
  UpdateBillingPortalMapper: {
    entity: Entities['UpdateBillingPortalMapper'];
    /** Optional configured preflight before creating a replacement provider session. */
    validateCachedUpdate?: (
      dto: Dto['UpdateBillingPortalMapper'],
      existing: Dto['BillingPortalMapper']
    ) => Promise<void>;
    /** Cache-only DTO update; preserve session/customer identities. */
    toCachedDto?: (
      dto: Dto['UpdateBillingPortalMapper'],
      existing: Dto['BillingPortalMapper'],
      em: EntityManager,
      stripeSession: Stripe.BillingPortal.Session,
      ...args: unknown[]
    ) => Promise<Dto['BillingPortalMapper']>;
    toEntity: (
      dto: Dto['UpdateBillingPortalMapper'],
      em: EntityManager,
      stripeSession: Stripe.BillingPortal.Session,
      ...args: unknown[]
    ) => Promise<InferEntity<Entities['UpdateBillingPortalMapper']>>;
  };
};
