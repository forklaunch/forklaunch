import { EntityManager, InferEntity } from '@mikro-orm/core';
import Stripe from 'stripe';
import { StripePaymentLinkDtos } from './stripe.dto.types';
import { StripePaymentLinkEntities } from './stripe.entity.types';

export type StripePaymentLinkMappers<
  StatusEnum,
  Entities extends StripePaymentLinkEntities,
  Dto extends StripePaymentLinkDtos<StatusEnum>
> = {
  PaymentLinkMapper: {
    /** Validate and restore declared DTO fields after a JSON cache read. */
    normalizeCachedDto?: (
      value: Dto['PaymentLinkMapper']
    ) => Dto['PaymentLinkMapper'];
    entity: Entities['PaymentLinkMapper'];
    toDto: (
      entity: InferEntity<Entities['PaymentLinkMapper']>
    ) => Promise<Dto['PaymentLinkMapper']>;
    validateCachedRead?: (existing: Dto['PaymentLinkMapper']) => void;
    /** Opt-in cached reference lookup; does not prove provider account ownership. */
    readCachedDto?: (
      existing: Dto['PaymentLinkMapper'],
      retrieve: (providerId: string) => Promise<Stripe.PaymentLink>
    ) => Promise<Dto['PaymentLinkMapper']>;
  };
  CreatePaymentLinkMapper: {
    /** Provider effects already occurred; validate shape before local persistence. */
    validateProviderCreate?: (provider: Stripe.PaymentLink) => Promise<void>;
    /** Pure configured validation before any provider creation effects. */
    validateCreate?: (dto: Dto['CreatePaymentLinkMapper']) => Promise<void>;
    entity: Entities['CreatePaymentLinkMapper'];
    toEntity: (
      dto: Dto['CreatePaymentLinkMapper'],
      em: EntityManager,
      stripePaymentLink: Stripe.PaymentLink,
      ...args: unknown[]
    ) => Promise<InferEntity<Entities['CreatePaymentLinkMapper']>>;
  };
  UpdatePaymentLinkMapper: {
    /** Validate the already-applied provider result and prepare one local update.
     * `provider.id` is the stored Stripe id, which the service has already
     * matched; `dto.id` stays the public id. Does not resolve aliases or
     * establish provider account ownership.
     */
    toLocalUpdate?: (
      dto: Dto['UpdatePaymentLinkMapper'],
      provider: Stripe.PaymentLink
    ) => Promise<Dto['UpdatePaymentLinkMapper']>;
    toCachedDto?: (
      dto: Dto['UpdatePaymentLinkMapper'],
      existing: Dto['PaymentLinkMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<Dto['PaymentLinkMapper']>;
    /** Validate a patch against the normalized cached DTO before provider I/O.
     * This hook does not resolve provider IDs or authorize provider mutations.
     */
    validateCachedUpdate?: (
      dto: Dto['UpdatePaymentLinkMapper'],
      existing: Dto['PaymentLinkMapper']
    ) => Promise<void>;
    entity: Entities['UpdatePaymentLinkMapper'];
    toEntity: (
      dto: Dto['UpdatePaymentLinkMapper'],
      em: EntityManager,
      stripePaymentLink: Stripe.PaymentLink,
      ...args: unknown[]
    ) => Promise<InferEntity<Entities['UpdatePaymentLinkMapper']>>;
  };
};
