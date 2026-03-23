import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityData, EntityManager, InferEntity, wrap } from '@mikro-orm/core';
import { Plan } from '../../persistence/entities/plan.entity';
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
  entity: Plan,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(Plan, {
        ...dto,
        providerFields: dto.providerFields ?? null
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
  entity: Plan,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      const entity = await em.findOneOrFail(Plan, { id: dto.id });
      em.assign(entity, { ...dto } as EntityData<InferEntity<typeof Plan>>);
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
  entity: Plan,
  mapperDefinition: {
    toDto: async (entity: InferEntity<typeof Plan>) => {
      return {
        ...wrap(entity).toPOJO(),
        price: Number(entity.price),
        description: entity.description ?? undefined,
        features: entity.features ?? undefined
      };
    }
  }
});
