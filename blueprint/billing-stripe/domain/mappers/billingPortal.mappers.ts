import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager } from '@mikro-orm/core';
import Stripe from 'stripe';
import { BillingPortal } from '../../persistence/entities/billingPortal.entity';
import { BillingPortalSchemas } from '../schemas';

export const CreateBillingPortalMapper = requestMapper({
  schemaValidator,
  schema: BillingPortalSchemas.CreateBillingPortalSchema,
  entity: BillingPortal,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.BillingPortal.Session
    ) => {
      return em.create(BillingPortal, {
        customerId: dto.customerId,
        uri: dto.uri ?? null,
        expiresAt: dto.expiresAt,
        providerFields
      });
    }
  }
});

export const UpdateBillingPortalMapper = requestMapper({
  schemaValidator,
  schema: BillingPortalSchemas.UpdateBillingPortalSchema,
  entity: BillingPortal,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.BillingPortal.Session
    ) => {
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { stripeFields, ...rest } = dto;
      const entity = await em.findOneOrFail(BillingPortal, { id: rest.id });
      em.assign(entity, {
        ...rest,
        providerFields
      });
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
        uri: entity.uri ?? undefined,
        stripeFields: entity.providerFields
      };
    }
  }
});
