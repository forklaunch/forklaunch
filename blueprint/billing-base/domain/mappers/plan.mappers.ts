import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { wrap } from '@mikro-orm/core';
import { EntityManager } from '@mikro-orm/core';
import { plan, type Plan } from '../../persistence/entities/plan.entity';
import { BillingProviderEnum } from '../enum/billingProvider.enum';
import { CurrencyEnum } from '../enum/currency.enum';
import { PlanCadenceEnum } from '../enum/planCadence.enum';
import { PlanSchemas } from '../schemas';

export const CreatePlanMapper = requestMapper({
  schemaValidator,
  schema: PlanSchemas.CreatePlanSchema(
    PlanCadenceEnum,
    CurrencyEnum,
    BillingProviderEnum
  ),
  entity: plan,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(plan, {
        ...dto,
        createdAt: new Date(),
        updatedAt: new Date()
      });
    }
  }
});

export const UpdatePlanMapper = requestMapper({
  schemaValidator,
  schema: PlanSchemas.UpdatePlanSchema(
    PlanCadenceEnum,
    CurrencyEnum,
    BillingProviderEnum
  ),
  entity: plan,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      const entity = await em.findOneOrFail(plan, { id: dto.id });
      em.assign(entity, { ...dto, updatedAt: new Date() });
      return entity;
    }
  }
});

export const PlanMapper = responseMapper({
  schemaValidator,
  schema: PlanSchemas.PlanSchema(
    PlanCadenceEnum,
    CurrencyEnum,
    BillingProviderEnum
  ),
  entity: plan,
  mapperDefinition: {
    toDto: async (entity: Plan) => {
      return wrap(entity).toPOJO();
    }
  }
});
