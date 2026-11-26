import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager } from '@mikro-orm/core';
import Stripe from 'stripe';
import { CheckoutSession } from '../../persistence/entities/checkoutSession.entity';
import { StatusEnum } from '../enum/status.enum';
import { CheckoutSessionSchemas } from '../schemas';
import { validateProviderCreation } from './providerCreation';

const CreateCheckoutSessionEntityMapper = requestMapper({
  schemaValidator,
  schema: CheckoutSessionSchemas.CreateCheckoutSessionSchema(StatusEnum),
  entity: CheckoutSession,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.Checkout.Session
    ) => {
      return em.create(CheckoutSession, {
        customerId: dto.customerId,
        paymentMethods: dto.paymentMethods,
        currency: dto.currency,
        uri: dto.uri ?? null,
        successRedirectUri: dto.successRedirectUri ?? null,
        cancelRedirectUri: dto.cancelRedirectUri ?? null,
        expiresAt: dto.expiresAt,
        status: dto.status,
        providerFields
      });
    }
  }
});

export const UpdateCheckoutSessionMapper = requestMapper({
  schemaValidator,
  schema: CheckoutSessionSchemas.UpdateCheckoutSessionSchema(StatusEnum),
  entity: CheckoutSession,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.Checkout.Session
    ) => {
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { stripeFields, ...rest } = dto;
      const entity = await em.findOneOrFail(CheckoutSession, { id: rest.id });
      em.assign(entity, {
        ...rest,
        providerFields
      });
      return entity;
    }
  }
});

const CheckoutSessionEntityMapper = responseMapper({
  schemaValidator,
  schema: CheckoutSessionSchemas.CheckoutSessionSchema(StatusEnum),
  entity: CheckoutSession,
  mapperDefinition: {
    toDto: async (entity) => {
      return {
        ...entity,
        uri: entity.uri ?? undefined,
        successRedirectUri: entity.successRedirectUri ?? undefined,
        cancelRedirectUri: entity.cancelRedirectUri ?? undefined,
        stripeFields: entity.providerFields
      };
    }
  }
});

/**
 * A reference from the server-written cache, not an account-ownership proof.
 * Managed account rebinding requires the separate verified gateway protocol.
 * This hook is read-only; mutation paths do not use it.
 */
function checkoutEvidence(value: unknown) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('Invalid cached checkout provider evidence');
  }
  const session = value as Record<string, unknown>;
  if (
    typeof session.id !== 'string' ||
    !/^cs_[A-Za-z0-9_]+$/.test(session.id) ||
    session.id.length > 128 ||
    session.object !== 'checkout.session' ||
    session.mode !== 'subscription' ||
    typeof session.livemode !== 'boolean'
  ) {
    throw new Error('Invalid cached checkout provider evidence');
  }
  const raw = session.customer;
  const customer =
    typeof raw === 'string' || raw === null
      ? raw
      : raw &&
          typeof raw === 'object' &&
          !Array.isArray(raw) &&
          (raw as Record<string, unknown>).object === 'customer' &&
          (!Object.hasOwn(raw, 'deleted') ||
            (raw as Record<string, unknown>).deleted === false)
        ? (raw as Record<string, unknown>).id
        : undefined;
  if (
    customer !== null &&
    (typeof customer !== 'string' ||
      !/^cus_[A-Za-z0-9_]+$/.test(customer) ||
      customer.length > 128)
  ) {
    throw new Error('Invalid cached checkout customer evidence');
  }
  return { id: session.id, livemode: session.livemode, customer };
}

export const CheckoutSessionMapper = {
  ...CheckoutSessionEntityMapper,
  readCachedDto: async (
    existing: Awaited<ReturnType<typeof CheckoutSessionEntityMapper.toDto>>,
    retrieve: (providerId: string) => Promise<Stripe.Checkout.Session>
  ) => {
    const parsed = schemaValidator.parse(
      schemaValidator.schemify(
        CheckoutSessionSchemas.CheckoutSessionSchema(StatusEnum)
      ),
      existing
    );
    if (!parsed.ok) throw new Error('Invalid cached checkout response');
    const before = checkoutEvidence(parsed.value.stripeFields);
    const session = await retrieve(before.id);
    const after = checkoutEvidence(session);
    if (
      after.id !== before.id ||
      after.livemode !== before.livemode ||
      (before.customer !== null && after.customer !== before.customer)
    ) {
      throw new Error('Checkout provider evidence does not match');
    }
    // https://docs.stripe.com/api/checkout/sessions/object: subscription checkout
    // can assign a customer during the payment flow when none was supplied.
    // Keep the application's public UUID/customer relation unchanged.
    return { ...existing, stripeFields: session };
  }
};

export const CreateCheckoutSessionMapper = {
  ...CreateCheckoutSessionEntityMapper,
  validateProviderCreate: async (provider: Stripe.Checkout.Session) => {
    validateProviderCreation(provider, 'checkout.session');
  },
  validateCreate: async (
    dto: Parameters<typeof CreateCheckoutSessionEntityMapper.toEntity>[0]
  ) => {
    // The service replaces these fields with provider or server-generated values.
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const { id, uri, expiresAt, ...inputSchema } =
      CheckoutSessionSchemas.CreateCheckoutSessionSchema(StatusEnum);
    const input = schemaValidator.parse(
      schemaValidator.schemify(inputSchema),
      dto
    );
    if (!input.ok) {
      throw new Error('Invalid checkoutSession creation');
    }
  }
};
