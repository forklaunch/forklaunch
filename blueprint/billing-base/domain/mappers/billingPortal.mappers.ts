import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager } from '@mikro-orm/core';
import { BillingPortal } from '../../persistence/entities/billingPortal.entity';
import { BillingPortalSchemas } from '../schemas';

export const CreateBillingPortalMapper = requestMapper({
  schemaValidator,
  schema: BillingPortalSchemas.CreateBillingPortalSchema,
  entity: BillingPortal,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(BillingPortal, {
        ...dto,
        providerFields: dto.providerFields ?? null
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
      em.assign(entity, { ...dto });
      return entity;
    }
  }
});

export const BillingPortalMapper = responseMapper({
  schemaValidator,
  schema: BillingPortalSchemas.BillingPortalSchema,
  entity: BillingPortal,
  mapperDefinition: {
    toDto: async (entity) => {
      return {
        ...entity,
        uri: entity.uri ?? undefined
      };
    }
  }
});
