import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { wrap } from '@mikro-orm/core';
import { EntityManager } from '@mikro-orm/core';
import Stripe from 'stripe';
import {
  billingPortal,
  type BillingPortal
} from '../../persistence/entities/billingPortal.entity';
import { BillingPortalSchemas } from '../schemas';

export const CreateBillingPortalMapper = requestMapper({
  schemaValidator,
  schema: BillingPortalSchemas.CreateBillingPortalSchema,
  entity: billingPortal,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.BillingPortal.Session
    ) => {
      return em.create(billingPortal, {
        customerId: dto.customerId,
        uri: dto.uri || null,
        expiresAt: dto.expiresAt,
        providerFields,
        createdAt: new Date(),
        updatedAt: new Date()
      });
    }
  }
});

export const UpdateBillingPortalMapper = requestMapper({
  schemaValidator,
  schema: BillingPortalSchemas.UpdateBillingPortalSchema,
  entity: billingPortal,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.BillingPortal.Session
    ) => {
      const entity = await em.findOneOrFail(billingPortal, { id: dto.id });
      em.assign(entity, {
        ...(dto.uri !== undefined && { uri: dto.uri }),
        ...(dto.expiresAt !== undefined && { expiresAt: dto.expiresAt }),
        providerFields,
        updatedAt: new Date()
      });
      return entity;
    }
  }
});

export const BillingPortalMapper = responseMapper({
  schemaValidator,
  schema: BillingPortalSchemas.BillingPortalSchema,
  entity: billingPortal,
  mapperDefinition: {
    toDto: async (entity: BillingPortal) => {
      return {
        ...wrap(entity).toPOJO(),
        stripeFields: entity.providerFields
      };
    }
  }
});
