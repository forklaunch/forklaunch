import { EntityManager, EntitySchema } from '@mikro-orm/core';
import Stripe from 'stripe';
import { StripePlanDtos } from './stripe.dto.types';
import { StripePlanEntities } from './stripe.entity.types';

export type StripePlanMappers<
  Entities extends StripePlanEntities,
  Dto extends StripePlanDtos
> = {
  PlanMapper: {
    entity: EntitySchema;
    toDto: (entity: Entities['PlanMapper']) => Promise<Dto['PlanMapper']>;
  };
  CreatePlanMapper: {
    entity: EntitySchema;
    toEntity: (
      dto: Dto['CreatePlanMapper'],
      em: EntityManager,
      stripePlan: Stripe.Product,
      ...args: unknown[]
    ) => Promise<Entities['CreatePlanMapper']>;
  };
  UpdatePlanMapper: {
    entity: EntitySchema;
    toEntity: (
      dto: Dto['UpdatePlanMapper'],
      em: EntityManager,
      stripePlan: Stripe.Product,
      ...args: unknown[]
    ) => Promise<Entities['UpdatePlanMapper']>;
  };
};
