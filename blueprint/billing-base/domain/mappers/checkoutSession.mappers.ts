import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager, InferEntity, wrap } from '@mikro-orm/core';
import { CheckoutSession } from '../../persistence/entities/checkoutSession.entity';
import { CurrencyEnum } from '../enum/currency.enum';
import { PaymentMethodEnum } from '../enum/paymentMethod.enum';
import { StatusEnum } from '../enum/status.enum';
import { CheckoutSessionSchemas } from '../schemas';

export const CreateCheckoutSessionMapper = requestMapper({
  schemaValidator,
  schema: CheckoutSessionSchemas.CreateCheckoutSessionSchema(
    PaymentMethodEnum,
    CurrencyEnum,
    StatusEnum
  ),
  entity: CheckoutSession,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(CheckoutSession, {
        ...dto,
        providerFields: dto.providerFields ?? null
      });
    }
  }
});

export const UpdateCheckoutSessionMapper = requestMapper({
  schemaValidator,
  schema: CheckoutSessionSchemas.UpdateCheckoutSessionSchema(
    PaymentMethodEnum,
    CurrencyEnum,
    StatusEnum
  ),
  entity: CheckoutSession,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      const entity = await em.findOneOrFail(CheckoutSession, { id: dto.id });
      em.assign(entity, { ...dto });
      return entity;
    }
  }
});

export const CheckoutSessionMapper = responseMapper({
  schemaValidator,
  schema: CheckoutSessionSchemas.CheckoutSessionSchema(
    PaymentMethodEnum,
    CurrencyEnum,
    StatusEnum
  ),
  entity: CheckoutSession,
  mapperDefinition: {
    toDto: async (entity: InferEntity<typeof CheckoutSession>) => {
      return {
        ...wrap(entity).toPOJO(),
        uri: entity.uri ?? undefined,
        successRedirectUri: entity.successRedirectUri ?? undefined,
        cancelRedirectUri: entity.cancelRedirectUri ?? undefined
      };
    }
  }
});
