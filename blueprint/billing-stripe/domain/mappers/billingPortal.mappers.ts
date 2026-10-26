import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager, wrap } from '@mikro-orm/core';
import Stripe from 'stripe';
import { BillingPortal } from '../../persistence/entities/billingPortal.entity';
import { BillingPortalSchemas } from '../schemas';
import { validateProviderCreation } from './providerCreation';

const CreateBillingPortalEntityMapper = requestMapper({
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

const UpdateBillingPortalEntityMapper = requestMapper({
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
      const pojo = wrap(entity).toPOJO();
      return {
        ...pojo,
        uri: entity.uri ?? undefined,
        stripeFields: entity.providerFields
      };
    }
  }
});

export const UpdateBillingPortalMapper = {
  ...UpdateBillingPortalEntityMapper,
  validateCachedUpdate: async (
    dto: Parameters<typeof UpdateBillingPortalEntityMapper.toEntity>[0],
    existing: Awaited<ReturnType<typeof BillingPortalMapper.toDto>>
  ) => {
    // These are trusted cache DTO identities, not coercible caller input.
    if (
      existing.id !== dto.id ||
      typeof existing.id !== 'string' ||
      !existing.id.trim() ||
      typeof existing.customerId !== 'string' ||
      !existing.customerId.trim()
    ) {
      throw new Error('Cached session identity cannot change');
    }
    const cached = schemaValidator.parse(
      schemaValidator.schemify(BillingPortalSchemas.BillingPortalSchema),
      existing
    );
    const patch = schemaValidator.parse(
      schemaValidator.schemify(BillingPortalSchemas.UpdateBillingPortalSchema),
      dto
    );
    if (!cached.ok || !patch.ok) {
      throw new Error('Invalid cached billing portal update');
    }
  },
  toCachedDto: async (
    dto: Parameters<typeof UpdateBillingPortalEntityMapper.toEntity>[0],
    existing: Awaited<ReturnType<typeof BillingPortalMapper.toDto>>,
    _em: EntityManager,
    providerFields: Stripe.BillingPortal.Session
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
    const updated = {
      ...existing,
      id: patch.value.id,
      ...(patch.value.uri === undefined ? {} : { uri: patch.value.uri }),
      ...(patch.value.expiresAt === undefined
        ? {}
        : { expiresAt: patch.value.expiresAt }),
      providerFields,
      stripeFields: providerFields
    };
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

export const CreateBillingPortalMapper = {
  ...CreateBillingPortalEntityMapper,
  validateProviderCreate: async (provider: Stripe.BillingPortal.Session) => {
    validateProviderCreation(provider, 'billing_portal.session');
    if (typeof provider.url !== 'string' || !provider.url.trim()) {
      throw new Error('Invalid Stripe billing response');
    }
  },
  validateCreate: async (
    dto: Parameters<typeof CreateBillingPortalEntityMapper.toEntity>[0]
  ) => {
    // The service replaces these fields with provider or server-generated values.
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const { id, uri, expiresAt, ...inputSchema } =
      BillingPortalSchemas.CreateBillingPortalSchema;
    const input = schemaValidator.parse(
      schemaValidator.schemify(inputSchema),
      dto
    );
    if (!input.ok) {
      throw new Error('Invalid billingPortal creation');
    }
  }
};
