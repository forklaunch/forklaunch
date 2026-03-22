import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager, InferEntity, wrap } from '@mikro-orm/core';
import { PaymentLink } from '../../persistence/entities/paymentLink.entity';
import { CurrencyEnum } from '../enum/currency.enum';
import { PaymentMethodEnum } from '../enum/paymentMethod.enum';
import { StatusEnum } from '../enum/status.enum';
import { PaymentLinkSchemas } from '../schemas';

export const CreatePaymentLinkMapper = requestMapper({
  schemaValidator,
  schema: PaymentLinkSchemas.CreatePaymentLinkSchema(
    PaymentMethodEnum,
    CurrencyEnum,
    StatusEnum
  ),
  entity: PaymentLink,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(PaymentLink, {
        ...dto,
        providerFields: dto.providerFields ?? null,
        createdAt: new Date(),
        updatedAt: new Date()
      });
    }
  }
});

export const UpdatePaymentLinkMapper = requestMapper({
  schemaValidator,
  schema: PaymentLinkSchemas.UpdatePaymentLinkSchema(
    PaymentMethodEnum,
    CurrencyEnum,
    StatusEnum
  ),
  entity: PaymentLink,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      const entity = await em.findOneOrFail(PaymentLink, { id: dto.id });
      em.assign(entity, { ...dto, updatedAt: new Date() });
      return entity;
    }
  }
});

export const PaymentLinkMapper = responseMapper({
  schemaValidator,
  schema: PaymentLinkSchemas.PaymentLinkSchema(
    PaymentMethodEnum,
    CurrencyEnum,
    StatusEnum
  ),
  entity: PaymentLink,
  mapperDefinition: {
    toDto: async (entity: InferEntity<typeof PaymentLink>) => {
      return {
        ...wrap(entity).toPOJO(),
        amount: Number(entity.amount)
      };
    }
  }
});
