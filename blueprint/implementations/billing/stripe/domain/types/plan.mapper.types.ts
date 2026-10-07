import { EntityManager, InferEntity } from '@mikro-orm/core';
import Stripe from 'stripe';
import { StripePlanDtos } from './stripe.dto.types';
import { StripePlanEntities } from './stripe.entity.types';

export type StripePlanMappers<
  Entities extends StripePlanEntities,
  Dto extends StripePlanDtos
> = {
  PlanMapper: {
    entity: Entities['PlanMapper'];
    toDto: (
      entity: InferEntity<Entities['PlanMapper']>
    ) => Promise<Dto['PlanMapper']>;
  };
  CreatePlanMapper: {
    entity: Entities['CreatePlanMapper'];
    toEntity: (
      dto: Dto['CreatePlanMapper'],
      em: EntityManager,
      stripePlan: Stripe.Product,
      ...args: unknown[]
    ) => Promise<InferEntity<Entities['CreatePlanMapper']>>;
  };
  UpdatePlanMapper: {
    entity: Entities['UpdatePlanMapper'];
    toEntity: (
      dto: Dto['UpdatePlanMapper'],
      em: EntityManager,
      stripePlan: Stripe.Product,
      ...args: unknown[]
    ) => Promise<InferEntity<Entities['UpdatePlanMapper']>>;
  };
};
