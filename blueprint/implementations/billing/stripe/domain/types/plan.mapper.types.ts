import { EntityManager, InferEntity } from '@mikro-orm/core';
import Stripe from 'stripe';
import { StripePlanDtos } from './stripe.dto.types';
import { StripePlanEntities } from './stripe.entity.types';

export type StripePlanMappers<
  Entities extends StripePlanEntities,
  Dto extends StripePlanDtos
> = {
  PlanMapper: {
    /** Read-result identity check; does not establish provider account ownership. */
    validateProviderRead?: (
      expectedExternalId: string,
      provider: Stripe.Plan
    ) => Promise<void>;
    entity: Entities['PlanMapper'];
    toDto: (
      entity: InferEntity<Entities['PlanMapper']>
    ) => Promise<Dto['PlanMapper']>;
  };
  CreatePlanMapper: {
    /** Provider effects already occurred; validate shape before local persistence. */
    validateProviderCreate?: (provider: Stripe.Plan) => Promise<void>;
    /** Pure configured validation before any provider creation effects. */
    validateCreate?: (dto: Dto['CreatePlanMapper']) => Promise<void>;
    entity: Entities['CreatePlanMapper'];
    toEntity: (
      dto: Dto['CreatePlanMapper'],
      em: EntityManager,
      stripePlan: Stripe.Product,
      ...args: unknown[]
    ) => Promise<InferEntity<Entities['CreatePlanMapper']>>;
  };
  UpdatePlanMapper: {
    /** Validate consumed provider references before a subsequent mutation. */
    validateProviderUpdate?: (
      dto: Dto['UpdatePlanMapper'],
      provider: Stripe.Plan
    ) => Promise<void>;
    /** Pure configured validation before any provider replacement effects. */
    validateUpdate?: (dto: Dto['UpdatePlanMapper']) => Promise<void>;
    entity: Entities['UpdatePlanMapper'];
    toEntity: (
      dto: Dto['UpdatePlanMapper'],
      em: EntityManager,
      stripePlan: Stripe.Product,
      ...args: unknown[]
    ) => Promise<InferEntity<Entities['UpdatePlanMapper']>>;
  };
};
