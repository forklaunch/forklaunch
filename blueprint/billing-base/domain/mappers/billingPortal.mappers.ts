import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager, wrap } from '@mikro-orm/core';
import {
  BillingPortal,
  type IBillingPortal
} from '../../persistence/entities/billingPortal.entity';
import { BillingPortalSchemas } from '../schemas';

export const CreateBillingPortalMapper = requestMapper({
  schemaValidator,
  schema: BillingPortalSchemas.CreateBillingPortalSchema,
  entity: BillingPortal,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(BillingPortal, {
        ...dto,
        createdAt: new Date(),
        updatedAt: new Date()
      });
    }
  }
});

export const UpdateBillingPortalMapper = requestMapper({
  schemaValidator,
  schema: BillingPortalSchemas.UpdateBillingPortalSchema,
  entity: BillingPortal,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      const entity = await em.findOneOrFail(BillingPortal, { id: dto.id });
      em.assign(entity, { ...dto, updatedAt: new Date() });
      return entity;
    }
  }
});

export const BillingPortalMapper = responseMapper({
  schemaValidator,
  schema: BillingPortalSchemas.BillingPortalSchema,
  entity: BillingPortal,
  mapperDefinition: {
    toDto: async (entity: IBillingPortal) => {
      return wrap(entity).toPOJO();
    }
  }
});
