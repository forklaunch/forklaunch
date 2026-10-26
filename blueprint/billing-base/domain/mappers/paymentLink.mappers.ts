import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager } from '@mikro-orm/core';
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
        providerFields: dto.providerFields ?? null
      });
    }
  }
});

const UpdatePaymentLinkEntityMapper = requestMapper({
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
      em.assign(entity, { ...dto });
      return entity;
    }
  }
});

const PaymentLinkEntityMapper = responseMapper({
  schemaValidator,
  schema: PaymentLinkSchemas.PaymentLinkSchema(
    PaymentMethodEnum,
    CurrencyEnum,
    StatusEnum
  ),
  entity: PaymentLink,
  mapperDefinition: {
    toDto: async (entity) => {
      return {
        ...entity,
        amount: Number(entity.amount)
      };
    }
  }
});

export const PaymentLinkMapper = {
  ...PaymentLinkEntityMapper,
  normalizeCachedDto: (
    value: Awaited<ReturnType<typeof PaymentLinkEntityMapper.toDto>>
  ) => {
    const parsed = schemaValidator.parse(
      schemaValidator.schemify(
        PaymentLinkSchemas.PaymentLinkSchema(
          PaymentMethodEnum,
          CurrencyEnum,
          StatusEnum
        )
      ),
      value
    );
    if (!parsed.ok) {
      throw new Error('Invalid cached payment link response');
    }
    return { ...value, ...parsed.value };
  }
};

export const UpdatePaymentLinkMapper = {
  ...UpdatePaymentLinkEntityMapper,
  toCachedDto: async (
    dto: Parameters<typeof UpdatePaymentLinkEntityMapper.toEntity>[0],
    existing: Awaited<ReturnType<typeof PaymentLinkMapper.toDto>>,
    _em: EntityManager
  ) => {
    const current = schemaValidator.parse(
      schemaValidator.schemify(
        PaymentLinkSchemas.PaymentLinkSchema(
          PaymentMethodEnum,
          CurrencyEnum,
          StatusEnum
        )
      ),
      existing
    );
    const patch = schemaValidator.parse(
      schemaValidator.schemify(
        PaymentLinkSchemas.UpdatePaymentLinkSchema(
          PaymentMethodEnum,
          CurrencyEnum,
          StatusEnum
        )
      ),
      dto
    );
    if (!current.ok || !patch.ok) {
      throw new Error('Invalid cached payment link update');
    }
    if (patch.value.id !== current.value.id) {
      throw new Error('Cached payment link identity cannot change');
    }
    const updated = { ...existing, ...current.value, ...patch.value };
    const result = schemaValidator.parse(
      schemaValidator.schemify(
        PaymentLinkSchemas.PaymentLinkSchema(
          PaymentMethodEnum,
          CurrencyEnum,
          StatusEnum
        )
      ),
      updated
    );
    if (!result.ok) {
      throw new Error('Invalid cached payment link response');
    }
    return { ...updated, ...result.value };
  }
};
