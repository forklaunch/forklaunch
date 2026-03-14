import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager, wrap } from '@mikro-orm/core';
import {
  CheckoutSession,
  type ICheckoutSession
} from '../../persistence/entities/checkoutSession.entity';
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
        createdAt: new Date(),
        updatedAt: new Date()
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
      em.assign(entity, { ...dto, updatedAt: new Date() });
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
    toDto: async (entity: ICheckoutSession) => {
      return wrap(entity).toPOJO();
    }
  }
});
