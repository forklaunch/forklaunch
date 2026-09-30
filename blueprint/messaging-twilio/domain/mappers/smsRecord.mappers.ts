import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { SmsStatusEnum } from '@forklaunch/implementation-messaging-twilio/enum';
import { EntityManager } from '@mikro-orm/core';
import { SmsRecord } from '../../persistence/entities/smsRecord.entity';
import { SmsSchemas } from '../schemas';

export const SendSmsMapper = requestMapper({
  schemaValidator,
  schema: SmsSchemas.SendSmsSchema,
  entity: SmsRecord,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(SmsRecord, {
        to: dto.to,
        body: dto.body,
        status: SmsStatusEnum.QUEUED,
        providerMessageId: null,
        error: null,
        metadata: dto.metadata ?? null
      });
    }
  }
});

export const SmsRecordMapper = responseMapper({
  schemaValidator,
  schema: SmsSchemas.SmsRecordSchema,
  entity: SmsRecord,
  mapperDefinition: {
    toDto: async (entity) => {
      return {
        ...entity,
        providerMessageId: entity.providerMessageId ?? undefined,
        error: entity.error ?? undefined,
        metadata:
          (entity.metadata ?? undefined) as
            | Record<string, unknown>
            | undefined
      };
    }
  }
});
