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

const UpdateBillingPortalEntityMapper = requestMapper({
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

export const UpdateBillingPortalMapper = {
  ...UpdateBillingPortalEntityMapper,
  toCachedDto: async (
    dto: Parameters<typeof UpdateBillingPortalEntityMapper.toEntity>[0],
    existing: Awaited<ReturnType<typeof BillingPortalMapper.toDto>>,
    _em: EntityManager
  ) => {
    const patch = schemaValidator.parse(
      schemaValidator.schemify(BillingPortalSchemas.UpdateBillingPortalSchema),
      dto
    );
    if (!patch.ok) {
      throw new Error('Invalid cached billing portal update');
    }
    if (patch.value.id !== existing.id) {
      throw new Error('Cached session identity cannot change');
    }
    const updated = { ...existing, ...patch.value };
    const result = schemaValidator.parse(
      schemaValidator.schemify(BillingPortalSchemas.BillingPortalSchema),
      updated
    );
    if (!result.ok) {
      throw new Error('Invalid cached billing portal response');
    }
    return updated;
  }
};
